const now = new Date();
const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
let region = null;
try { const candidate = new Intl.Locale(navigator.language).region; region = /^[A-Z]{2}$/.test(candidate ?? "") ? candidate : null; } catch (_) {}
(async () => {
    const controller = new AbortController();
    let timer;
    try {
        const country = await Promise.race([
            (async () => {
                const response = await fetch(__LOOKUP_URL__, {
                    credentials: "omit", cache: "no-store", referrerPolicy: "no-referrer", signal: controller.signal,
                });
                if (!response.ok) return null;
                const payload = await response.json();
                return typeof payload?.country === "string" && /^[A-Z]{2}$/.test(payload.country)
                    ? payload.country : null;
            })(),
            new Promise(resolve => { timer = setTimeout(() => resolve(null), __TIMEOUT_MS__); }),
        ]);
        region = country ?? region;
    } catch (_) {
    } finally {
        clearTimeout(timer);
        controller.abort();
    }
    dioxus.send([today, region]);
})();
