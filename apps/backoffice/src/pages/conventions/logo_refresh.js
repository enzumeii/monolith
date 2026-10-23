let timer;
let dueAt;
let lastRefresh = 0;
const clear = () => { clearTimeout(timer); timer = undefined; };
const refresh = () => {
    clear();
    const delay = Math.max(0, lastRefresh + 60000 - Date.now());
    timer = setTimeout(() => {
        lastRefresh = Date.now();
        dioxus.send(true);
    }, delay);
};
const onFocus = () => { if (dueAt !== undefined && Date.now() >= dueAt) refresh(); };
window.addEventListener('focus', onFocus);
try {
    while (true) {
        const schedule = await dioxus.recv();
        if (schedule === null) break;
        clear();
        const [expiry, failed] = schedule;
        if (!expiry && !failed) { dueAt = undefined; continue; }
        const parsed = expiry ? Date.parse(expiry) : NaN;
        dueAt = failed ? Date.now() : Number.isFinite(parsed) ? parsed - 30000 : Date.now() + 60000;
        timer = setTimeout(refresh, Math.max(0, dueAt - Date.now()));
    }
} finally {
    clear();
    window.removeEventListener('focus', onFocus);
}
