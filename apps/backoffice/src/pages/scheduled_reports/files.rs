use dioxus::prelude::*;
use domain::dto::ReportFileDTO;
use std::time::Duration;

#[component]
pub fn ReportFiles(files: Vec<ReportFileDTO>, #[props(default)] compact: bool) -> Element {
    rsx! {
        if files.is_empty() { p { "Files unavailable" } }
        else {
            ul { class: if compact { "report-files report-files-compact" } else { "report-files" },
                for (index,file) in files.iter().enumerate() {
                    SavedFile { key: "{index}:{file.download_url}", file: file.clone(), compact }
                }
            }
        }
    }
}

pub fn eligible_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

#[derive(Default)]
pub struct Activation {
    last: Option<Duration>,
}
impl Activation {
    pub fn begin(&mut self, now: Duration) -> bool {
        if self
            .last
            .is_some_and(|last| now.saturating_sub(last) < Duration::from_millis(500))
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

#[component]
fn SavedFile(file: ReportFileDTO, compact: bool) -> Element {
    let started = use_hook(web_time::Instant::now);
    let mut activation = use_signal(Activation::default);
    rsx! {
        li {
            if eligible_url(&file.download_url) {
                a { href: file.download_url.clone(), target: "_blank", rel: "noopener noreferrer",
                    class: if compact { "report-file-download" } else { "" },
                    aria_label: "Download {file.filename} ({file.content_type})",
                    title: "{file.filename} · {file.content_type}",
                    onclick: move |event| {
                        if !activation.write().begin(started.elapsed()) { event.prevent_default(); }
                    },
                    if compact {
                        svg { width: "20", height: "20", view_box: "0 0 24 24", fill: "none",
                            stroke: "currentColor", stroke_width: "1.75", stroke_linecap: "round", stroke_linejoin: "round",
                            "aria-hidden": "true", "focusable": "false",
                            path { d: "M12 3v12m-5-5 5 5 5-5M5 16v4h14v-4" }
                        }
                    } else { "{file.filename}" }
                }
            } else { span { "{file.filename} · File link unavailable" } }
            if !compact { span { " · {file.content_type}" } }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/files.rs"]
mod tests;
