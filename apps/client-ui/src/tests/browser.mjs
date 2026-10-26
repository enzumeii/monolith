import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const visitor = readFileSync(new URL('../cart/visitor.js', import.meta.url), 'utf8');
const restore = readFileSync(new URL('../cart/restore.js', import.meta.url), 'utf8');

async function locate(language, fetch, timeout = 30, DateImpl = Date) {
    const script = visitor.replace('__LOOKUP_URL__', JSON.stringify('https://country.example.test/')).replace('__TIMEOUT_MS__', String(timeout));
    return new Promise((resolve, reject) => {
        try {
            runInNewContext(script, { Date: DateImpl, String, Intl, AbortController, setTimeout, clearTimeout, fetch, navigator: { language }, dioxus: { send: resolve } });
        } catch (error) { reject(error); }
    });
}

for (const [language, expected] of [['th-TH', 'TH'], ['en-US', 'US'], ['en', null], ['es-419', null], ['en-001', null], ['invalid_locale', null]]) {
    test(`visitor falls back to explicit alpha2 region: ${language}`, async () => {
        const actual = await locate(language, async () => { throw new Error('offline'); });
        assert.equal(actual[1], expected);
        assert.match(actual[0], /^\d{4}-\d{2}-\d{2}$/);
    });
}

for (const [language, country] of [['en-US', 'TH'], ['th-TH', 'AU']]) {
    test(`visitor prefers IP ${country} over language ${language}`, async () => {
        const calls = [];
        const actual = await locate(language, async (...args) => { calls.push(args); return { ok: true, json: async () => ({ country, ip: 'discard-me' }) }; });
        assert.equal(actual[1], country);
        assert.equal(actual.length, 2);
        assert.equal(calls.length, 1);
        assert.equal(calls[0][0], 'https://country.example.test/');
        assert.equal(calls[0][1].credentials, 'omit');
        assert.equal(calls[0][1].cache, 'no-store');
        assert.equal(calls[0][1].signal.aborted, true);
        assert.equal(calls[0][1].referrerPolicy, 'no-referrer');
    });
}

for (const country of [null, '', '419', 'THAILAND', 'th', 42]) {
    test(`invalid IP country ${country} preserves locale fallback`, async () => {
        assert.equal((await locate('en-US', async () => ({ ok: true, json: async () => ({ country }) })))[1], 'US');
    });
}

test('HTTP, malformed body and timeout failures preserve fallback', async () => {
    for (const fetch of [
        async () => ({ ok: false }),
        async () => ({ ok: true, json: async () => { throw new Error('invalid JSON'); } }),
        async () => new Promise(() => {}),
        async () => ({ ok: true, json: async () => new Promise(() => {}) }),
    ]) {
        assert.equal((await locate('th-TH', fetch, 5))[1], 'TH');
    }
});

test('a late lookup cannot publish another country after the timeout fallback', async () => {
    let resolveLookup;
    let signal;
    const actual = await locate('th-TH', async (_url, options) => {
        signal = options.signal;
        return new Promise(resolve => { resolveLookup = resolve; });
    }, 5);
    assert.equal(actual[1], 'TH');
    assert.equal(signal.aborted, true);
    resolveLookup({ ok: true, json: async () => ({ country: 'AU' }) });
    await new Promise(resolve => setTimeout(resolve, 5));
    assert.equal(actual[1], 'TH');
});

test('visitor uses local calendar date across a UTC day boundary', async () => {
    class LocalDate { getFullYear() { return 2026; } getMonth() { return 9; } getDate() { return 24; } }
    const actual = await locate('en', async () => { throw new Error('offline'); }, 30, LocalDate);
    assert.equal(actual[0], '2026-10-24');
});

test('blocked browser storage returns a recoverable state and browser-generated checkout ID', () => {
    let actual;
    runInNewContext(restore.replace('__STORAGE_KEY__', '"event"'), {
        localStorage: { getItem() { throw new Error('blocked'); } },
        crypto: { randomUUID() { return 'f79b7e64-8e36-4aca-9e21-ecb7cb513700'; } },
        dioxus: { send(value) { actual = value; } },
    });
    assert.equal(actual.blocked, true);
    assert.equal(actual.raw, null);
    assert.equal(actual.checkout_id, 'f79b7e64-8e36-4aca-9e21-ecb7cb513700');
});


test('cart focus refreshes once per foreground transition and removes listeners on unmount', async () => {
    const source = readFileSync(new URL('../cart/focus.js', import.meta.url), 'utf8');
    const windowListeners = new Map();
    const documentListeners = new Map();
    let focused = true;
    let release;
    const received = [];
    const window = {
        addEventListener: (type, listener) => windowListeners.set(type, listener),
        removeEventListener: type => windowListeners.delete(type),
    };
    const document = {
        visibilityState: 'visible', hasFocus: () => focused,
        addEventListener: (type, listener) => documentListeners.set(type, listener),
        removeEventListener: type => documentListeners.delete(type),
    };
    const task = runInNewContext(source, { window, document, dioxus: { send: value => received.push(value), recv: () => new Promise(resolve => { release = resolve; }) } });
    assert.equal(received.length, 0);
    focused = false;
    windowListeners.get('blur')();
    document.visibilityState = 'hidden';
    documentListeners.get('visibilitychange')();
    document.visibilityState = 'visible';
    documentListeners.get('visibilitychange')();
    focused = true;
    windowListeners.get('focus')();
    documentListeners.get('visibilitychange')();
    assert.deepEqual(received, [true]);
    release(false);
    await task;
    assert.equal(windowListeners.size, 0);
    assert.equal(documentListeners.size, 0);
});

const cartSave = readFileSync(new URL('../cart/save.js', import.meta.url), 'utf8');
const cartId = 'f79b7e64-8e36-4aca-9e21-ecb7cb513700';
const nextCartId = 'b65bbebd-f990-4599-bc72-daa653abc900';
const cartRecord = id => ({version: 1, convention_id: 'event', checkout_id: id, items: []});
async function saveCartScript(current, proposed, persist, beforeLock, holdRequest = false) {
    let raw = current && JSON.stringify(current);
    const messages = [];
    const locks = [];
    let written = 0;
    const script = cartSave
        .replace('__STORAGE_KEY__', JSON.stringify('enzumeii.cart.v1.event'))
        .replace('__CONVENTION_ID__', JSON.stringify('event'))
        .replace('__CART_JSON__', JSON.stringify(JSON.stringify(proposed)));
    const task = runInNewContext(`(async () => {${script}})()`, {
        localStorage: {
            getItem() { return raw; },
            setItem(_key, value) { written++; raw = value; },
        },
        navigator: {locks: {async request(name, work) { locks.push(name); if (holdRequest && name === 'enzumeii.checkout.event') await new Promise(() => {}); if (beforeLock) raw = JSON.stringify(beforeLock); await work(); }}},
        dioxus: {send: value => messages.push(value), recv: async () => persist},
    });
    await task;
    return {cart: JSON.parse(raw), messages, locks, written};
}

test('stale cart save cannot restore the identity abandoned by another tab', async () => {
    const current = {...cartRecord(nextCartId), items: [{variant_id: 'newer', quantity: 3}]};
    const actual = await saveCartScript(current, cartRecord(cartId), false);
    assert.deepEqual(actual.cart, current);
    assert.equal(actual.written, 0);
});

test('cart save reads and writes under its short cart lock', async () => {
    const actual = await saveCartScript(cartRecord(cartId), cartRecord(cartId), true);
    assert.deepEqual(actual.locks, ['enzumeii.cart.v1.event']);
    assert.equal(actual.written, 1);
});

test('queued cart save inspects the identity persisted before lock acquisition', async () => {
    const current = cartRecord(nextCartId);
    const actual = await saveCartScript(cartRecord(cartId), cartRecord(cartId), false, current);
    assert.deepEqual(actual.cart, current);
    assert.equal(actual.messages[0].raw, JSON.stringify(current));
});


test('a new cart edit persists while checkout request ownership is held', async () => {
    const proposed = {...cartRecord(cartId), items: [{variant_id: 'newer', quantity: 3}]};
    const actual = await Promise.race([
        saveCartScript(cartRecord(cartId), proposed, true, null, true),
        new Promise((_resolve, reject) => setTimeout(() => reject(new Error('cart edit waited for payment request')), 100)),
    ]);
    assert.deepEqual(actual.cart, proposed);
    assert.equal(actual.written, 1);
});
