use crate::{api, export, session};
use dioxus::prelude::*;
use domain::dto::ReportFileDTO;

#[component]
pub fn ReportFiles(
    report_id: String,
    files: Vec<ReportFileDTO>,
    #[props(default)] compact: bool,
) -> Element {
    rsx! {
        if files.is_empty() { p { "Files unavailable" } }
        else {
            ul { class: if compact { "report-files report-files-compact" } else { "report-files" },
                for file in &files {
                    SavedFile { key: "{report_id}:{file.filename}", report_id: report_id.clone(), file: file.clone(), compact }
                }
            }
        }
    }
}

#[component]
fn SavedFile(report_id: String, file: ReportFileDTO, compact: bool) -> Element {
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let label = if busy() {
        "Preparing download"
    } else if error().is_some() {
        "Retry download"
    } else {
        "Download"
    };
    rsx! {
        li {
            button { r#type: "button", disabled: busy(), aria_busy: busy(),
                class: if compact { "report-file-download" } else { "ghost" },
                aria_label: "{label} {file.filename} ({file.content_type})",
                title: "{file.filename} · {file.content_type}",
                onclick: move |_| {
                    if busy() { return; }
                    busy.set(true);
                    error.set(None);
                    let report_id = report_id.clone();
                    let filename = file.filename.clone();
                    spawn(async move {
                            let _loading = ui::loading_guard();
                        let result = async {
                            let signed = session::with_session(|| api::report_results::get_download_url(&report_id, &filename))
                                .await.map_err(|error| error.to_string())?;
                            export::download_url(&signed.url).await
                        }.await;
                        busy.set(false);
                        if let Err(message) = result { error.set(Some(message)); }
                    });
                },
                if compact {
                    svg { width: "20", height: "20", view_box: "0 0 24 24", fill: "none",
                        stroke: "currentColor", stroke_width: "1.75", stroke_linecap: "round", stroke_linejoin: "round",
                        "aria-hidden": "true", "focusable": "false",
                        path { d: "M12 3v12m-5-5 5 5 5-5M5 16v4h14v-4" }
                    }
                } else if busy() { "Preparing download…" }
                else if error().is_some() { "Retry download {file.filename}" }
                else { "{file.filename}" }
            }
            if !compact { span { " · {file.content_type}" } }
            if busy() { span { role: "status", "Preparing {file.filename}…" } }
            if let Some(message) = error() {
                p { role: "alert", "Unable to download {file.filename}: {message}. Retry the download." }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/files.rs"]
mod tests;
