import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { webcrypto } from 'node:crypto';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../../checkout/bridge.js', import.meta.url), 'utf8');
const cart = {version:1, convention_id:'event', checkout_id:'cart', items:[{variant_id:'variant',quantity:1,accepted_unit_price:'12.30'}]};
const input = {action:'start', slug:'convention', convention_id:'event', snapshot:cart, provider:'stripe', currency:'USD', total:'12.30', payload:'{"operation_id":"","email":"buyer@example.test","items":[{"expected_unit_price":9007199254740993}]}'};
function storage(map = new Map()) {
    const listeners = new Set();
    const changed = key => queueMicrotask(() => { for (const listener of listeners) listener(key); });
    return {
        getItem: key => map.get(key) ?? null,
        setItem(key,value) { map.set(key,value); changed(key); },
        removeItem(key) { map.delete(key); changed(key); },
        connect(listener) { listeners.add(listener); },
        map,
    };
}
function locks() {
    const names = new Map();
    return {request(name, options, callback) {
        if (typeof options === 'function') { callback = options; options = {}; }
        const state = names.get(name) || {held:false, queue:[]};
        names.set(name,state);
        return new Promise((resolve,reject) => {
            const entry = {callback,resolve,reject,signal:options.signal,aborted:false};
            const next = () => {
                if (state.held) return;
                const job = state.queue.shift();
                if (!job) return;
                if (job.aborted) { next(); return; }
                state.held = true;
                Promise.resolve().then(() => job.callback({name})).then(job.resolve,job.reject).finally(() => {state.held=false; next();});
            };
            entry.signal?.addEventListener('abort', () => {
                entry.aborted = true;
                entry.reject(new DOMException('Aborted','AbortError'));
                next();
            }, {once:true});
            state.queue.push(entry);
            if (entry.signal?.aborted) { entry.aborted=true; reject(new DOMException('Aborted','AbortError')); }
            next();
        });
    }};
}
function browserWindow(local) {
    const target = new EventTarget();
    local.connect?.(key => { const event=new Event('storage'); event.key=key; target.dispatchEvent(event); });
    return target;
}
function begin(input, localStorage = storage(), navigator = {locks:locks()}, sessionStorage = storage()) {
    let resolveBoot, resolveCommand;
    const boot = new Promise(resolve => { resolveBoot = resolve; });
    const command = new Promise(resolve => { resolveCommand = resolve; });
    const messages = [];
    const context = {localStorage, sessionStorage, navigator, window:browserWindow(localStorage), Event, crypto:webcrypto, Uint8Array, dioxus:{
        send(message) { messages.push(message); if (messages.length === 1) resolveBoot(message); },
        recv() { return command; },
    }};
    const done = runInNewContext(`(async () => {${source.replace('__INPUT__', JSON.stringify(input))}})()`, context);
    return {boot, done, finish: status => resolveCommand({status}), messages, localStorage, sessionStorage};
}

test('persists random secret and identity before request, keeps email only in session payload, and preserves integer digits', async () => {
    const run = begin(input);
    const boot = await run.boot;
    assert.equal(boot.ok, true);
    assert.equal(boot.value.fresh, true);
    assert.match(boot.value.attempt.secret, /^[a-f0-9]{64}$/);
    assert.match(boot.value.attempt.operation_id, /^[a-f0-9-]{36}$/);
    assert.equal(boot.value.attempt.state, 'submitted');
    assert.ok(boot.value.payload.includes('9007199254740993'));
    assert.ok(boot.value.payload.includes(boot.value.attempt.operation_id));
    assert.ok(!JSON.stringify([...run.localStorage.map]).includes('buyer@example.test'));
    assert.ok(JSON.stringify([...run.sessionStorage.map]).includes('buyer@example.test'));
    run.finish('processing'); await run.done;
    assert.equal(JSON.parse(run.localStorage.getItem(`enzumeii.checkout.v1.${boot.value.attempt.operation_id}`)).state, 'processing');
});

test('two simultaneous tabs share one immutable attempt and never create a second payment', async () => {
    const local = storage(); const navigator = {locks:locks()};
    const first = begin(input, local, navigator);
    const second = begin({...input, provider:'paypal'}, local, navigator);
    const firstBoot = await first.boot;
    let secondStarted = false;
    second.boot.then(() => { secondStarted = true; });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(secondStarted, false);
    first.finish('processing'); await first.done;
    const secondBoot = await second.boot;
    assert.equal(secondBoot.value.fresh, false);
    assert.equal(secondBoot.value.attempt.operation_id, firstBoot.value.attempt.operation_id);
    assert.equal(secondBoot.value.attempt.secret, firstBoot.value.attempt.secret);
    assert.equal(secondBoot.value.attempt.provider, 'stripe');
    assert.equal(secondBoot.value.payload, null);
    second.finish(null); await second.done;
});

test('unavailable atomic ownership or required persistence prevents any network handoff', async () => {
    for (const [navigator, local] of [[{},storage()], [{locks:locks()}, {getItem(){return null;},setItem(){throw new Error('denied');}}]]) {
        const run = begin(input, local, navigator);
        const boot = await run.boot;
        assert.equal(boot.ok, false);
        assert.equal(boot.value, undefined);
        await run.done;
    }
});

for (const modified of [false,true]) {
    test(`completion clears only the exact saved cart snapshot (newer edit=${modified})`, async () => {
        const local = storage();
        const current = modified ? {...cart, items:[{...cart.items[0], quantity:2}]} : cart;
        local.setItem('enzumeii.cart.v1.event', JSON.stringify(current));
        const run = begin(input, local);
        const boot = await run.boot;
        run.finish('completed'); await run.done;
        const saved = JSON.parse(local.getItem('enzumeii.cart.v1.event'));
        assert.equal(saved.items.length, modified ? 1 : 0);
        if (modified) assert.equal(saved.items[0].quantity, 2);
        assert.equal(run.sessionStorage.getItem(`enzumeii.checkout.payload.v1.${boot.value.attempt.operation_id}`), null);
        assert.equal(run.messages[1].value, !modified);
    });
}

// Cart::save serializes struct declaration order; serde_json::json! sorts snapshot keys.
for (const [ordering, snapshot] of [
    ['cart fields', {checkout_id:'cart', convention_id:'event', items:cart.items, version:1}],
    ['item fields', {...cart, items:[{accepted_unit_price:'12.30', quantity:1, variant_id:'variant'}]}],
    ['cart and item fields', {checkout_id:'cart', convention_id:'event', items:[{accepted_unit_price:'12.30', quantity:1, variant_id:'variant'}], version:1}],
]) {
    test(`completion clears an unchanged cart when IPC reorders ${ordering}`, async () => {
        const local = storage();
        local.setItem('enzumeii.cart.v1.event', JSON.stringify(cart));
        const run = begin({...input, snapshot}, local);
        await run.boot;
        run.finish('completed'); await run.done;
        const saved = JSON.parse(local.getItem('enzumeii.cart.v1.event'));
        assert.deepEqual(saved.items, []);
        assert.notEqual(saved.checkout_id, 'cart');
        assert.equal(run.messages[1].value, true);
    });
}

test('completion preserves every meaningful cart edit and unrecognized schema field', async () => {
    const second = {variant_id:'second', quantity:1, accepted_unit_price:'2.00'};
    for (const [change, current, snapshot] of [
        ['version', {...cart, version:2}, cart],
        ['convention', {...cart, convention_id:'another'}, cart],
        ['checkout identity', {...cart, checkout_id:'newer-cart'}, cart],
        ['item identity', {...cart, items:[{...cart.items[0], variant_id:'another'}]}, cart],
        ['quantity', {...cart, items:[{...cart.items[0], quantity:2}]}, cart],
        ['price', {...cart, items:[{...cart.items[0], accepted_unit_price:'12.31'}]}, cart],
        ['item order', {...cart, items:[second, ...cart.items]}, {...cart, items:[...cart.items, second]}],
        ['unknown cart field', {...cart, extra:true}, cart],
        ['unknown item field', {...cart, items:[{...cart.items[0], extra:true}]}, cart],
        ['shared unknown cart field', {...cart, extra:true}, {...cart, extra:true}],
        ['shared unknown item field', {...cart, items:[{...cart.items[0], extra:true}]}, {...cart, items:[{...cart.items[0], extra:true}]}],
    ]) {
        const local = storage();
        const raw = JSON.stringify(current);
        const run = begin({...input, snapshot}, local);
        await run.boot;
        local.setItem('enzumeii.cart.v1.event', raw);
        run.finish('completed'); await run.done;
        assert.equal(local.getItem('enzumeii.cart.v1.event'), raw, change);
        assert.equal(run.messages[1].value, false, change);
    }
});

test('cancel/reload preserves an ambiguous attempt and its exact original payload', async () => {
    const local = storage(); const session = storage(); const navigator = {locks:locks()};
    const first = begin(input,local,navigator,session); const boot = await first.boot;
    first.finish(null); await first.done;
    const second = begin({...input,action:'recover',id:boot.value.attempt.operation_id},local,navigator,session);
    const recovered = await second.boot;
    assert.equal(recovered.value.attempt.operation_id, boot.value.attempt.operation_id);
    assert.equal(recovered.value.fresh,false);
    assert.equal(recovered.value.payload,boot.value.payload);
    second.finish(null); await second.done;
});

test('missing or differently scoped recovery records never disclose another convention checkout', async () => {
    const missing = begin({action:'read',slug:'convention',id:'missing'});
    assert.equal((await missing.boot).value,null); await missing.done;
    const local = storage(); const first = begin(input,local); const boot = await first.boot;
    first.finish('processing'); await first.done;
    const wrong = begin({action:'read',slug:'another-convention',id:boot.value.attempt.operation_id},local);
    assert.equal((await wrong.boot).ok,false); await wrong.done;
});

test('neutral cancellation release preserves submitted uncertainty and lets a second tab acquire the same attempt', async () => {
    const local=storage(); const navigator={locks:locks()}; const session=storage();
    const first=begin(input,local,navigator,session); const boot=await first.boot;
    const second=begin(input,local,navigator,storage());
    first.finish(null); await first.done;
    const next=await second.boot;
    assert.equal(next.value.attempt.operation_id,boot.value.attempt.operation_id);
    assert.equal(next.value.attempt.state,'submitted');
    assert.equal(next.value.fresh,false);
    assert.ok(session.getItem(`enzumeii.checkout.payload.v1.${boot.value.attempt.operation_id}`));
    second.finish(null); await second.done;
});

test('notes stay only in the immutable session request and recover without replacement', async () => {
    const note = '  Pack flat.\nขอบคุณ 🐕 ';
    const withNote = {...input, payload:JSON.stringify({operation_id:'',email:'buyer@example.test',note})};
    const run = begin(withNote);
    const started = await run.boot;
    assert.equal(JSON.parse(started.value.payload).note, note);
    assert.ok(!JSON.stringify([...run.localStorage.map]).includes('Pack flat'));
    assert.ok(JSON.stringify([...run.sessionStorage.map]).includes('Pack flat'));
    run.finish(null); await run.done;
    const recovered = begin({...input,action:'recover',id:started.value.attempt.operation_id},run.localStorage,{locks:locks()},run.sessionStorage);
    const boot = await recovered.boot;
    assert.equal(boot.value.fresh,false);
    assert.equal(boot.value.payload,started.value.payload);
    assert.equal(JSON.parse(boot.value.payload).note,note);
    recovered.finish(null); await recovered.done;
});

function paymentWindow({blocked = false, closed = false, refuseNavigation = false, localStorage = storage()} = {}) {
    let receive;
    const events = [];
    const messages = [];
    const command = new Promise(resolve => { receive = resolve; });
    const popup = {
        closed,
        opener: {},
        location: {replace(url) { if (refuseNavigation) throw new Error('navigation denied'); events.push(['navigate', url]); }},
        focus() { events.push(['focus']); },
        close() { this.closed = true; events.push(['close']); },
    };
    const script = readFileSync(new URL('../../checkout/payment-window.js', import.meta.url), 'utf8');
    const done = runInNewContext(`(async () => {${script}})()`, {
        window: {open(...args) { events.push(['open', ...args]); return blocked ? null : popup; }},
        localStorage,
        dioxus: {send(message) { messages.push(message); }, recv() { return command; }},
    });
    return {popup, events, messages, done, localStorage, command: value => receive(value)};
}

test('payment context is reserved before any await and handed off without replacing the original page or opener access', async () => {
    const run = paymentWindow();
    assert.deepEqual(run.events, [['open', 'about:blank', '_blank']]);
    assert.equal(run.popup.opener, null);
    assert.equal(run.messages[0].ok, true);
    run.command({action:'open',checkout_id:'operation',url:'https://checkout.stripe.com/c/pay/test'});
    await run.done;
    assert.deepEqual(run.events, [['open', 'about:blank', '_blank'], ['navigate','https://checkout.stripe.com/c/pay/test'], ['focus']]);
    assert.equal(run.messages[1].ok, true);
    assert.equal(run.popup.closed, false);
});

test('blocked and closed payment contexts report recovery errors without navigating a fallback', async () => {
    const blocked = paymentWindow({blocked:true});
    await blocked.done;
    assert.equal(blocked.messages[0].ok, false);
    assert.match(blocked.messages[0].error, /allow.*popup|allow.*pop-up/i);
    const closed = paymentWindow();
    closed.popup.closed = true;
    closed.command({action:'open',checkout_id:'operation',url:'https://www.paypal.com/checkoutnow?token=test'});
    await closed.done;
    assert.equal(closed.messages[1].ok, false);
    assert.match(closed.messages[1].error, /closed|open/i);
    assert.equal(closed.events.filter(event => event[0] === 'navigate').length, 0);
});

test('dropping an unused reservation closes it and failed handoff closes only its blank context', async () => {
    const abort = paymentWindow();
    abort.command({action:'abort'});
    await abort.done;
    assert.equal(abort.popup.closed, true);
    assert.equal(abort.events.filter(event => event[0] === 'navigate').length, 0);
    const failed = paymentWindow({refuseNavigation:true});
    failed.command({action:'open',checkout_id:'operation',url:'https://checkout.stripe.com/test'});
    await failed.done;
    assert.equal(failed.messages[1].ok, false);
    assert.equal(failed.popup.closed, true);
});

async function createdAttempt(local = storage(), navigator = {locks:locks()}, session = storage()) {
    local.setItem('enzumeii.cart.v1.event',JSON.stringify(cart));
    const run=begin(input,local,navigator,session);
    const {value:{attempt}}=await run.boot;
    run.finish('processing'); await run.done;
    return {attempt,local,navigator,session};
}
function abandon(attempt, local, navigator, session=storage()) {
    return begin({action:'abandon',slug:attempt.slug,id:attempt.operation_id,convention_id:attempt.snapshot.convention_id},local,navigator,session);
}
const tick = () => new Promise(resolve => setImmediate(resolve));

test('abandonment preserves current items, forks identity once, and permits a distinct new order without resume',async () => {
    const {attempt,local,navigator,session}=await createdAttempt();
    const edited={...cart,items:[{...cart.items[0],quantity:2}]};
    local.setItem('enzumeii.cart.v1.event',JSON.stringify(edited));
    const cancel=abandon(attempt,local,navigator,session);
    const result=await cancel.boot;
    assert.equal(result.ok,true);
    assert.deepEqual(JSON.parse(JSON.stringify(result.value.items)),edited.items);
    await cancel.done;
    assert.notEqual(result.value.checkout_id,cart.checkout_id);
    assert.equal(local.getItem('enzumeii.checkout.active.v1.event'),null);
    assert.equal(JSON.parse(local.getItem(`enzumeii.checkout.v1.${attempt.operation_id}`)).state,'processing');
    assert.equal(local.getItem(`enzumeii.checkout.ignored.v1.${attempt.operation_id}`),'true');
    assert.equal(session.getItem(`enzumeii.checkout.payload.v1.${attempt.operation_id}`),null);
    const again=abandon(attempt,local,navigator); const repeated=await again.boot; await again.done;
    assert.equal(repeated.value.checkout_id,result.value.checkout_id);
    const next=begin({...input,snapshot:result.value},local,navigator);
    const boot=await next.boot;
    assert.equal(boot.value.fresh,true);
    assert.notEqual(boot.value.attempt.operation_id,attempt.operation_id);
    next.finish(null); await next.done;
    const stale=abandon(attempt,local,navigator); await stale.boot; await stale.done;
    assert.equal(local.getItem('enzumeii.checkout.active.v1.event'),boot.value.attempt.operation_id);
});

test('ignore is published before waiting for an in-flight request and late completion cannot clear its new cart',async () => {
    const local=storage(); const navigator={locks:locks()};
    local.setItem('enzumeii.cart.v1.event',JSON.stringify(cart));
    const first=begin(input,local,navigator); const {value:{attempt}}=await first.boot;
    const queued=begin({...input,action:'recover',id:attempt.operation_id},local,navigator);
    const cancel=abandon(attempt,local,navigator,first.sessionStorage);
    assert.equal(local.getItem(`enzumeii.checkout.ignored.v1.${attempt.operation_id}`),'true');
    first.finish('completed'); await first.done;
    assert.equal((await queued.boot).ok,false); await queued.done;
    const result=await cancel.boot; await cancel.done;
    assert.deepEqual(JSON.parse(JSON.stringify(result.value.items)),cart.items);
    assert.notEqual(result.value.checkout_id,cart.checkout_id);
    assert.equal(first.messages[1].value,false);
    const oldRead=begin({action:'read',slug:attempt.slug,convention_id:'event'},local,navigator);
    assert.equal((await oldRead.boot).value,null); await oldRead.done;
    const explicit=begin({action:'read',slug:attempt.slug,id:attempt.operation_id},local,navigator);
    assert.equal((await explicit.boot).value.operation_id,attempt.operation_id); await explicit.done;
    const replay=begin({...input,action:'recover',id:attempt.operation_id},local,navigator,first.sessionStorage);
    assert.equal((await replay.boot).ok,false); await replay.done;
});

test('abandonment is scoped, verifies persistence, preserves a newer cart and rejects stale fresh-start snapshots',async () => {
    const {attempt,local,navigator}=await createdAttempt();
    const newer={...cart,checkout_id:'newer-cart',items:[]};
    local.setItem('enzumeii.cart.v1.event',JSON.stringify(newer));
    const wrong=abandon({...attempt,slug:'another'},local,navigator);
    assert.equal((await wrong.boot).ok,false); await wrong.done;
    const cancel=abandon(attempt,local,navigator); const result=await cancel.boot; await cancel.done;
    assert.deepEqual(JSON.parse(JSON.stringify(result.value)),newer);
    const stale=begin(input,local,navigator);
    assert.equal((await stale.boot).ok,false); await stale.done;
    const denied={...local,setItem(key,value){if(key.includes('.ignored.'))throw new Error('denied');local.setItem(key,value);}};
    const failure=abandon(attempt,denied,navigator);
    assert.equal((await failure.boot).ok,false); await failure.done;
});

function checking(attempt, local, navigator) {
    const source=readFileSync(new URL('../../checkout/checking-session.js',import.meta.url),'utf8');
    const messages=[]; let release;
    const command=new Promise(resolve=>{release=resolve;});
    const done=runInNewContext(`(async()=>{${source.replace('__INPUT__',JSON.stringify({id:attempt.operation_id,slug:attempt.slug}))}})()`,{
        localStorage:local,navigator,window:browserWindow(local),AbortController,Event,
        dioxus:{send(message){messages.push(message);},recv(){return command;}},
    });
    return {messages,done,release:()=>release({action:'release'})};
}

test('one checking tab owns a payment while return page waits, then takes over automatically',async () => {
    const {attempt,local,navigator}=await createdAttempt();
    const first=checking(attempt,local,navigator); await tick();
    const second=checking(attempt,local,navigator); await tick();
    assert.deepEqual(first.messages.map(x=>x.value),['passive','owner']);
    assert.deepEqual(second.messages.map(x=>x.value),['passive']);
    first.release(); await first.done; await tick();
    assert.deepEqual(second.messages.map(x=>x.value),['passive','owner']);
    second.release(); await second.done;
});

test('abandonment notifies owner and passive tabs and queued disposal never steals ownership',async () => {
    const {attempt,local,navigator}=await createdAttempt();
    const first=checking(attempt,local,navigator); await tick();
    const disposed=checking(attempt,local,navigator); await tick(); disposed.release(); await disposed.done;
    const second=checking(attempt,local,navigator); await tick();
    const cancel=abandon(attempt,local,navigator); await cancel.boot; await cancel.done;
    await Promise.all([first.done,second.done]);
    assert.equal(first.messages.at(-1).value,'ignored');
    assert.equal(second.messages.at(-1).value,'ignored');
    assert.equal(disposed.messages.filter(x=>x.value==='owner').length,0);
    const reopened=checking(attempt,local,navigator); await reopened.done;
    assert.deepEqual(reopened.messages.map(x=>x.value),['ignored']);
});

test('partial cancellation is safely retryable and does not rotate an already forked cart again', async () => {
    const {attempt,local,navigator}=await createdAttempt();
    let readbackFailure=false;
    const partial={...local,
        setItem(key,value){local.setItem(key,value); if(key==='enzumeii.cart.v1.event')readbackFailure=true;},
        getItem(key){if(key==='enzumeii.cart.v1.event'&&readbackFailure){readbackFailure=false;return null;}return local.getItem(key);},
    };
    const failed=abandon(attempt,partial,navigator);
    assert.equal((await failed.boot).ok,false); await failed.done;
    assert.equal(local.getItem(`enzumeii.checkout.ignored.v1.${attempt.operation_id}`),'true');
    const forked=JSON.parse(local.getItem('enzumeii.cart.v1.event'));
    assert.notEqual(forked.checkout_id,cart.checkout_id);
    assert.equal(local.getItem('enzumeii.checkout.active.v1.event'),attempt.operation_id);
    const oldWork=begin(input,local,navigator);
    assert.equal((await oldWork.boot).ok,false); await oldWork.done;
    const retry=abandon(attempt,local,navigator); const recovered=await retry.boot; await retry.done;
    assert.equal(recovered.ok,true);
    assert.equal(recovered.value.checkout_id,forked.checkout_id);
    assert.equal(local.getItem('enzumeii.checkout.active.v1.event'),null);
});

test('ignored old callbacks can repeat abandonment without changing a newer active order',async () => {
    const {attempt,local,navigator}=await createdAttempt();
    const cancel=abandon(attempt,local,navigator); const {value:forked}=await cancel.boot; await cancel.done;
    const next=begin({...input,snapshot:forked},local,navigator); const boot=await next.boot;
    next.finish('processing'); await next.done;
    const before=[...local.map];
    for(const [action,expected] of [['ignored',true],['abandon',forked]]) {
        const read=begin({action,slug:attempt.slug,id:attempt.operation_id},local,navigator);
        const result=await read.boot; await read.done;
        assert.equal(result.ok,true);
        assert.deepEqual(JSON.parse(JSON.stringify(result.value)),JSON.parse(JSON.stringify(expected)));
    }
    assert.deepEqual([...local.map],before);
    assert.equal(local.getItem('enzumeii.checkout.active.v1.event'),boot.value.attempt.operation_id);
});

test('disposing a checking session before its first event prevents ownership and later waiters still acquire',async () => {
    const {attempt,local,navigator}=await createdAttempt();
    const owner=checking(attempt,local,navigator); await tick();
    const disposed=checking(attempt,local,navigator); disposed.release(); await disposed.done;
    owner.release(); await owner.done;
    const successor=checking(attempt,local,navigator); await tick();
    assert.deepEqual(successor.messages.map(x=>x.value),['passive','owner']);
    successor.release(); await successor.done;
});


test('completion waits for a short cart edit and preserves the edited snapshot', async () => {
    const local = storage(); const navigator = {locks:locks()};
    local.setItem('enzumeii.cart.v1.event', JSON.stringify(cart));
    const run = begin(input, local, navigator); await run.boot;
    let releaseEdit;
    const editing = navigator.locks.request('enzumeii.cart.v1.event', async () => {
        await new Promise(resolve => { releaseEdit = resolve; });
        local.setItem('enzumeii.cart.v1.event', JSON.stringify({...cart, items:[{...cart.items[0], quantity:2}]}));
    });
    await tick();
    run.finish('completed'); await tick();
    assert.equal(run.messages.length, 1, 'completion waits for the cart write');
    releaseEdit(); await editing; await run.done;
    const saved = JSON.parse(local.getItem('enzumeii.cart.v1.event'));
    assert.equal(saved.items[0].quantity, 2);
    assert.equal(saved.checkout_id, cart.checkout_id);
    assert.equal(run.messages[1].value, false);
});

test('ignore published while completion waits prevents all old cart and active-pointer effects', async () => {
    const local = storage(); const navigator = {locks:locks()};
    local.setItem('enzumeii.cart.v1.event', JSON.stringify(cart));
    const run = begin(input, local, navigator); const {value:{attempt}} = await run.boot;
    let releaseEdit;
    const editing = navigator.locks.request('enzumeii.cart.v1.event', () => new Promise(resolve => { releaseEdit = resolve; }));
    await tick();
    run.finish('completed'); await tick();
    assert.equal(run.messages.length, 1);
    local.setItem(`enzumeii.checkout.ignored.v1.${attempt.operation_id}`, 'true');
    releaseEdit(); await editing; await run.done;
    assert.equal(local.getItem('enzumeii.cart.v1.event'), JSON.stringify(cart));
    assert.equal(local.getItem('enzumeii.checkout.active.v1.event'), attempt.operation_id);
    assert.equal(run.messages[1].value, false);
});

test('abandonment waits for a current cart edit before forking its identity', async () => {
    const {attempt, local, navigator} = await createdAttempt();
    let releaseEdit;
    const editing = navigator.locks.request('enzumeii.cart.v1.event', async () => {
        await new Promise(resolve => { releaseEdit = resolve; });
        local.setItem('enzumeii.cart.v1.event', JSON.stringify({...cart, items:[{...cart.items[0], quantity:3}]}));
    });
    await tick();
    const cancel = abandon(attempt, local, navigator);
    await tick();
    assert.equal(cancel.messages.length, 0, 'abandonment waits for the short cart write');
    releaseEdit(); await editing;
    const result = await cancel.boot; await cancel.done;
    assert.equal(result.value.items[0].quantity, 3);
    assert.notEqual(result.value.checkout_id, cart.checkout_id);
});

test('a cancellation published after reservation prevents the actual provider handoff', async () => {
    const run = paymentWindow();
    run.localStorage.setItem('enzumeii.checkout.ignored.v1.operation', 'true');
    run.command({action:'open', checkout_id:'operation', url:'https://checkout.stripe.com/c/pay/test'});
    await run.done;
    assert.equal(run.events.filter(event => event[0] === 'navigate').length, 0);
    assert.equal(run.popup.closed, true);
    assert.equal(run.messages[1].ok, false);
});
