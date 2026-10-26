const input = __INPUT__;
const ignoredKey = `enzumeii.checkout.ignored.v1.${input.id}`;
const controller = typeof AbortController === 'function' ? new AbortController() : null;
let release;
let stopped = false;
const held = new Promise(resolve => { release = resolve; });
const send = state => dioxus.send({ok: true, value: state});
const stop = () => {
    stopped = true;
    controller?.abort();
    release();
};
function observeIgnore() {
    try {
        if (localStorage.getItem(ignoredKey) === 'true' && !stopped) {
            send('ignored');
            stop();
        }
    } catch (_) {
        dioxus.send({ok: false, error: 'Unable to watch this checkout safely. Check browser storage before paying again.'});
        stop();
    }
}
try {
    const raw = localStorage.getItem(`enzumeii.checkout.v1.${input.id}`);
    const attempt = raw && JSON.parse(raw);
    if (!attempt || attempt.version !== 1 || attempt.operation_id !== input.id || attempt.slug !== input.slug
        || !navigator.locks || !controller) throw new Error('Checking ownership unavailable');
    window.addEventListener('storage', observeIgnore);
    window.addEventListener('enzumeii-checkout-change', observeIgnore);
    observeIgnore();
    if (!stopped) {
        send('passive');
        const disposed = dioxus.recv().then(stop, stop);
        try {
            await navigator.locks.request(`enzumeii.checkout.checking.v1.${input.id}`, {signal: controller.signal}, async () => {
                observeIgnore();
                if (stopped) return;
                send('owner');
                await held;
            });
        } catch (error) {
            if (!stopped || error.name !== 'AbortError') throw error;
        }
        void disposed;
    }
} catch (_) {
    if (!stopped) dioxus.send({ok: false, error: 'Unable to safely coordinate payment checking in this browser. Keep any receipt and contact the shop before paying again.'});
    stop();
} finally {
    window.removeEventListener('storage', observeIgnore);
    window.removeEventListener('enzumeii-checkout-change', observeIgnore);
}
