const navigation = window.navigation;
const supported = !!navigation?.currentEntry && typeof navigation.traverseTo === 'function';
let dirty = false;
let prefix = '';
let sourceKey = null;
let restoring = null;
let recoveryRequired = false;
let destination = null;
let stopped = false;
let revision = 0;

function routeOf(url) {
    const parsed = new URL(url);
    let path = parsed.pathname;
    if (prefix && path.startsWith(prefix)) path = path.slice(prefix.length) || '/';
    return path + parsed.search;
}

function restoreSource() {
    if (!dirty || stopped || restoring || !sourceKey) return;
    const ticket = revision;
    restoring = sourceKey;
    recoveryRequired = true;
    dioxus.send({ restoration: true });
    navigation.traverseTo(sourceKey).finished.then(() => {
        if (!stopped && dirty && ticket === revision) {
            restoring = null;
            recoveryRequired = false;
            dioxus.send({ restoration: false, ...destination });
        }
    }).catch(() => {
        restoring = null;
        if (!stopped && ticket === revision) dioxus.send({ restoration: true, error: 'Unable to restore the editor history entry. Retry restoration before continuing. Your draft remains open.' });
    });
}

function onPopState(event) {
    if (!dirty || !supported || stopped) return;
    const entry = navigation.currentEntry;
    if (entry.key === restoring) {
        event.stopImmediatePropagation();
        return;
    }
    if (entry.key === sourceKey) return;
    if (recoveryRequired) {
        event.stopImmediatePropagation();
        restoreSource();
        return;
    }
    const route = routeOf(entry.url);
    if (route.split('?')[0] === '/scheduled-reports') {
        sourceKey = entry.key;
        return;
    }
    event.stopImmediatePropagation();
    destination = { key: entry.key, route };
    restoreSource();
}

window.addEventListener('popstate', onPopState, true);
try {
    while (!stopped) {
        const command = await dioxus.recv();
        switch (command.action) {
            case 'observe':
                prefix = command.prefix || '';
                dirty = command.dirty;
                if (!restoring && !recoveryRequired) revision += 1;
                if (dirty && supported && !restoring && !recoveryRequired) sourceKey = navigation.currentEntry.key;
                if (!dirty) { revision += 1; recoveryRequired = false; restoring = null; destination = null; }
                if (dirty && !supported) dioxus.send({ error: 'This browser cannot protect unsaved changes during Back or Forward navigation. Save or close the editor before using browser navigation.' });
                break;
            case 'retry_restore':
                if (recoveryRequired) restoreSource();
                break;
            case 'cancel':
                if (!recoveryRequired) destination = null;
                break;
            case 'confirm':
                if (recoveryRequired) { restoreSource(); break; }
                dirty = false;
                revision += 1;
                try {
                    await navigation.traverseTo(command.key).finished;
                } catch {
                    dirty = true;
                    if (navigation.currentEntry.key !== sourceKey) restoreSource();
                    else dioxus.send({ error: 'Unable to open that history entry. Your draft remains open.' });
                }
                break;
            case 'stop':
                stopped = true;
                dirty = false;
                revision += 1;
                break;
        }
    }
} finally {
    stopped = true;
    window.removeEventListener('popstate', onPopState, true);
}
