let raw = null;
let blocked = false;
try { raw = localStorage.getItem(__STORAGE_KEY__); } catch (_) { blocked = true; }
dioxus.send({raw, blocked, checkout_id: crypto.randomUUID()});
