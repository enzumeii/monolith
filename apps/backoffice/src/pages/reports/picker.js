const [id, suggestedName] = await dioxus.recv();
const active = () => document.getElementById(id)?.isConnected === true;
let writable;
try {
    if (!active()) { dioxus.send({kind: 'cancelled'}); return; }
    if (typeof window.showSaveFilePicker !== 'function') {
        dioxus.send({kind: 'unsupported'});
        return;
    }
    const handle = await window.showSaveFilePicker({
        suggestedName,
        excludeAcceptAllOption: true,
        types: [
            {description: 'CSV document', accept: {'text/csv': ['.csv']}},
            {description: 'Excel workbook', accept: {'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': ['.xlsx']}}
        ]
    });
    if (!active()) { dioxus.send({kind: 'cancelled'}); return; }
    dioxus.send({kind: 'selected', name: handle.name});
    const encoded = await dioxus.recv();
    if (encoded === null || !active()) { return; }
    writable = await handle.createWritable();
    if (!active()) { await writable.abort(); return; }
    await writable.write(Uint8Array.from(atob(encoded), character => character.charCodeAt(0)));
    if (!active()) { await writable.abort(); return; }
    await writable.close();
    writable = undefined;
    dioxus.send({kind: 'saved'});
} catch (error) {
    if (writable) { try { await writable.abort(); } catch (_) {} }
    if (error.name === 'AbortError' || !active()) {
        dioxus.send({kind: 'cancelled'});
    } else {
        dioxus.send({kind: 'failed', message: 'Unable to save the report. Check file permissions and try again.'});
    }
}
