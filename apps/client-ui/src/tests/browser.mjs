import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const visitor = readFileSync(new URL('../cart/visitor.js', import.meta.url), 'utf8');
const restore = readFileSync(new URL('../cart/restore.js', import.meta.url), 'utf8');

for (const [language, expected] of [['th-TH', 'TH'], ['en-US', 'US'], ['en', null], ['es-419', null], ['en-001', null], ['invalid_locale', null]]) {
    test(`visitor uses only explicit alpha2 region: ${language}`, () => {
        let actual;
        runInNewContext(visitor, { Date, String, Intl, navigator: { language }, dioxus: { send(value) { actual = value; } } });
        assert.equal(actual[1], expected);
        assert.match(actual[0], /^\d{4}-\d{2}-\d{2}$/);
    });
}

test('visitor uses local calendar date across a UTC day boundary', () => {
    class LocalDate { getFullYear() { return 2026; } getMonth() { return 9; } getDate() { return 24; } }
    let actual;
    runInNewContext(visitor, { Date: LocalDate, String, Intl, navigator: { language: 'en' }, dioxus: { send(value) { actual = value; } } });
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
