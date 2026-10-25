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
