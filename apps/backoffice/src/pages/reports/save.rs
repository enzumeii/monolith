use super::export::{ExportFormat, encode_report};
use crate::components::ErrorMessage;
use base64::Engine;
use dioxus::prelude::*;
use domain::dto::ProductReportRow;
use serde::Deserialize;
use std::{
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_SAVE: AtomicUsize = AtomicUsize::new(1);

#[component]
pub fn SaveReport(rows: Vec<ProductReportRow>, filename: String) -> Element {
    let id = use_hook(|| format!("report-save-{}", NEXT_SAVE.fetch_add(1, Ordering::Relaxed)));
    let mut busy = use_signal(|| false);
    let mut open = use_signal(|| false);
    let mut name = use_signal(|| filename.clone());
    let mut format = use_signal(|| ExportFormat::Csv);
    let mut error = use_signal(|| None::<String>);
    let snapshot = Rc::new(rows);
    let close = use_callback({
        let id = id.clone();
        move |()| {
            open.set(false);
            error.set(None);
            focus_save(&id);
        }
    });
    rsx! {
        div { id: "{id}", class: "report-save",
            button { class: "ghost", disabled: busy() || open() || snapshot.is_empty(),
                onclick: {
                    let id = id.clone();
                    let rows = snapshot.clone();
                    move |_| {
                        if rows.is_empty() || busy() || open() { return; }
                        error.set(None);
                        busy.set(true);
                        let picker = begin_picker(&id, &filename);
                        let rows = rows.clone();
                        let filename = filename.clone();
                        spawn(async move {
                            match finish_picker(picker, &rows).await {
                                Ok(true) => { name.set(filename); format.set(ExportFormat::Csv); open.set(true); }
                                Ok(false) => {},
                                Err(message) => error.set(Some(message)),
                            }
                            busy.set(false);
                        });
                    }
                },
                if busy() { "Saving…" } else { "Save report…" }
            }
            if !open() { ErrorMessage { message: error() } }
            if open() {
                div { class: "modal-overlay",
                    div { id: "{id}-dialog", class: "modal save-report-dialog", role: "dialog", aria_modal: "true", aria_labelledby: "{id}-title",
                        onmounted: {
                            let dialog_id = format!("{id}-dialog");
                            move |_| focus_dialog(&dialog_id)
                        },
                        onkeydown: move |event| {
                            if event.key() == Key::Escape && !busy() { event.prevent_default(); close(()); }
                        },
                        h3 { id: "{id}-title", class: "modal-title", "Save report as" }
                        form {
                            onsubmit: {
                                let id = id.clone();
                                let rows = snapshot.clone();
                                move |event| {
                                    event.prevent_default();
                                    if busy() || rows.is_empty() { return; }
                                    let selected = format();
                                    let filename = match selected.filename(&name()) {
                                        Ok(filename) => filename,
                                        Err(message) => { error.set(Some(message)); return; }
                                    };
                                    error.set(None); busy.set(true);
                                    let rows = rows.clone(); let id = id.clone();
                                    spawn(async move {
                                        let saved = match encode_report(&rows, selected) {
                                            Ok(bytes) => download(bytes, &filename, selected, &id).await,
                                            Err(message) => Err(message),
                                        };
                                        busy.set(false);
                                        match saved { Ok(()) => close(()), Err(message) => error.set(Some(message)) }
                                    });
                                }
                            },
                            label { r#for: "{id}-filename", "Filename" }
                            input { id: "{id}-filename", value: "{name}", required: true, disabled: busy(),
                                oninput: move |event| { name.set(event.value()); error.set(None); }
                            }
                            label { r#for: "{id}-format", "File type" }
                            select { id: "{id}-format", value: format().extension(), disabled: busy(),
                                onchange: move |event| {
                                    let selected = if event.value() == "xlsx" { ExportFormat::Xlsx } else { ExportFormat::Csv };
                                    format.set(selected); error.set(None);
                                    if let Ok(filename) = selected.filename(&name()) { name.set(filename); }
                                },
                                option { value: "csv", "CSV (.csv)" }
                                option { value: "xlsx", "Excel workbook (.xlsx)" }
                            }
                            p { "Your browser controls the save location. To choose a folder each time, open its Downloads settings and enable “Ask where to save files before downloading” (Zen) or “Always ask you where to save files” (Firefox)." }
                            ErrorMessage { message: error() }
                            div { class: "modal-actions",
                                button { r#type: "button", class: "ghost", disabled: busy(), onclick: move |_| close(()), "Cancel" }
                                button { r#type: "submit", class: "primary", disabled: busy(), if busy() { "Saving…" } else { "Save" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PickerReply {
    Unsupported,
    Selected { name: String },
    Cancelled,
    Saved,
    Failed { message: String },
}

fn begin_picker(id: &str, filename: &str) -> Result<document::Eval, String> {
    let picker = document::eval(include_str!("picker.js"));
    picker
        .send((id, filename))
        .map_err(|_| "Unable to open Save As".to_string())?;
    Ok(picker)
}

async fn finish_picker(
    picker: Result<document::Eval, String>,
    rows: &[ProductReportRow],
) -> Result<bool, String> {
    let mut picker = picker?;
    match picker
        .recv::<PickerReply>()
        .await
        .map_err(|_| "Unable to open Save As".to_string())?
    {
        PickerReply::Unsupported => return Ok(true),
        PickerReply::Cancelled => return Ok(false),
        PickerReply::Failed { message } => return Err(message),
        PickerReply::Selected { name } => {
            let encoded =
                ExportFormat::from_filename(&name).and_then(|format| encode_report(rows, format));
            match encoded {
                Ok(bytes) => picker
                    .send(Some(encoded_bytes(&bytes)))
                    .map_err(|_| "Unable to save report".to_string())?,
                Err(message) => {
                    let _ = picker.send(None::<String>);
                    return Err(message);
                }
            }
        }
        PickerReply::Saved => return Err("Unexpected Save As response".into()),
    }
    match picker
        .recv::<PickerReply>()
        .await
        .map_err(|_| "Unable to save report".to_string())?
    {
        PickerReply::Saved | PickerReply::Cancelled => Ok(false),
        PickerReply::Failed { message } => Err(message),
        _ => Err("Unexpected Save As response".into()),
    }
}

pub(crate) async fn download(
    bytes: Vec<u8>,
    filename: &str,
    format: ExportFormat,
    id: &str,
) -> Result<(), String> {
    let mut download = document::eval(
        r"
        const [encoded, filename, mime, id] = await dioxus.recv();
        if (!document.getElementById(id)?.isConnected) { dioxus.send(false); return; }
        const url = URL.createObjectURL(new Blob([Uint8Array.from(atob(encoded), character => character.charCodeAt(0))], {type: mime}));
        const link = document.createElement('a');
        link.href = url; link.download = filename;
        document.body.appendChild(link); link.click(); link.remove();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        dioxus.send(true);
    ",
    );
    download
        .send((encoded_bytes(&bytes), filename, format.mime(), id))
        .map_err(|_| "Unable to save report".to_string())?;
    download
        .recv::<bool>()
        .await
        .map_err(|_| "Unable to save report".to_string())?;
    Ok(())
}

fn focus_save(id: &str) {
    let focus = document::eval(
        "const id = await dioxus.recv(); document.getElementById(id)?.querySelector('button')?.focus();",
    );
    let _ = focus.send(id);
}
fn focus_dialog(id: &str) {
    let focus = document::eval(
        r"
        const id = await dioxus.recv();
        const dialog = document.getElementById(id);
        if (!dialog) return;
        dialog.querySelector('input')?.focus();
        dialog.addEventListener('keydown', event => {
            if (event.key !== 'Tab') return;
            const controls = [...dialog.querySelectorAll('input, select, button')].filter(control => !control.disabled);
            const first = controls[0], last = controls[controls.length - 1];
            if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
            else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
        });
    ",
    );
    let _ = focus.send(id);
}

fn encoded_bytes(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
#[cfg(test)]
#[path = "../../tests/pages/reports/save.rs"]
mod tests;
