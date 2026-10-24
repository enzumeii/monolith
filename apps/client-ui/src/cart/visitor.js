const now = new Date();
const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
let region = null;
try { const candidate = new Intl.Locale(navigator.language).region; region = /^[A-Z]{2}$/.test(candidate ?? "") ? candidate : null; } catch (_) {}
dioxus.send([today, region]);
