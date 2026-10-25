import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { webcrypto } from 'node:crypto';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../../checkout/bridge.js', import.meta.url), 'utf8');
const cart = {version:1, convention_id:'event', checkout_id:'cart', items:[{variant_id:'variant',quantity:1,accepted_unit_price:'12.30'}]};
const input = {action:'start', slug:'convention', convention_id:'event', snapshot:cart, provider:'stripe', currency:'USD', total:'12.30', payload:'{"operation_id":"","email":"buyer@example.test","items":[{"expected_unit_price":9007199254740993}]}'};
function storage(map = new Map()) { return {getItem: key => map.get(key) ?? null, setItem: (key,value) => map.set(key,value), removeItem: key => map.delete(key), map}; }
function locks() {
    let queued = Promise.resolve();
    return {request(_name, callback) { const current = queued.then(callback); queued = current.catch(() => {}); return current; }};
}
function begin(input, localStorage = storage(), navigator = {locks:locks()}, sessionStorage = storage()) {
    let resolveBoot, resolveCommand;
    const boot = new Promise(resolve => { resolveBoot = resolve; });
    const command = new Promise(resolve => { resolveCommand = resolve; });
    const messages = [];
    const context = {localStorage, sessionStorage, navigator, crypto:webcrypto, Uint8Array, dioxus:{
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
        local.setItem('enzumeii.cart.v1.event', raw);
        const run = begin({...input, snapshot}, local);
        await run.boot;
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
