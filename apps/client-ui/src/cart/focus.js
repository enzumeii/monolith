(async () => {
    let active = document.visibilityState === "visible" && document.hasFocus();
    const changed = () => {
        const next = document.visibilityState === "visible" && document.hasFocus();
        if (next && !active) dioxus.send(true);
        active = next;
    };
    window.addEventListener("focus", changed);
    window.addEventListener("blur", changed);
    document.addEventListener("visibilitychange", changed);
    try { await dioxus.recv(); }
    finally {
        window.removeEventListener("focus", changed);
        window.removeEventListener("blur", changed);
        document.removeEventListener("visibilitychange", changed);
    }
})();
