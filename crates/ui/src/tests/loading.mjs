import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const script = readFileSync(new URL('../loading.js', import.meta.url), 'utf8');

function paint() {
    const frames = new Map();
    const timers = new Map();
    const messages = [];
    let next = 0;
    const promise = runInNewContext(`(async () => { ${script} })()`, {
        requestAnimationFrame(callback) { const id = ++next; frames.set(id, callback); return id; },
        cancelAnimationFrame(id) { frames.delete(id); },
        setTimeout(callback, delay) { const id = ++next; timers.set(id, { callback, delay }); return id; },
        clearTimeout(id) { timers.delete(id); },
        dioxus: { send(value) { messages.push(value); } },
    });
    const frame = () => {
        assert.ok(frames.size > 0, 'a frame is pending');
        const [id, callback] = frames.entries().next().value;
        frames.delete(id);
        callback(0);
    };
    const timeout = () => {
        assert.equal(timers.size, 1, 'paint has a bounded fallback while frames are suspended');
        const [id, { callback, delay }] = timers.entries().next().value;
        assert.ok(delay > 0 && delay <= 100, 'painting must not hold an operation indefinitely');
        timers.delete(id);
        callback();
    };
    return { promise, frames, timers, messages, frame, timeout };
}

test('loading paint yields two visible frames and clears its fallback', async () => {
    const pending = paint();
    assert.deepEqual(pending.messages, []);
    pending.frame();
    assert.deepEqual(pending.messages, []);
    pending.frame();
    await pending.promise;
    assert.deepEqual(pending.messages, [true]);
    assert.equal(pending.frames.size, 0);
    assert.equal(pending.timers.size, 0);
});

test('suspended animation frames cannot strand a loading operation', async () => {
    const pending = paint();
    pending.timeout();
    await pending.promise;
    assert.deepEqual(pending.messages, [true]);
    assert.equal(pending.frames.size, 0);
    assert.equal(pending.timers.size, 0);
});

test('frames suspended after the first callback also release from the timeout', async () => {
    const pending = paint();
    pending.frame();
    pending.timeout();
    await pending.promise;
    assert.deepEqual(pending.messages, [true]);
    assert.equal(pending.frames.size, 0);
    assert.equal(pending.timers.size, 0);
});
