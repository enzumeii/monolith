use dioxus::prelude::*;
use domain::dto::ReportFileDTO;
use std::time::Duration;

#[component]
pub fn ReportFiles(files: Vec<ReportFileDTO>) -> Element {
    rsx! {
        if files.is_empty() { p { "Files unavailable" } }
        else {
            ul { class: "report-files",
                for (index,file) in files.iter().enumerate() {
                    SavedFile { key: "{index}:{file.download_url}", file: file.clone() }
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
fn SavedFile(file: ReportFileDTO) -> Element {
    let started = use_hook(web_time::Instant::now);
    let mut activation = use_signal(Activation::default);
    rsx! {
        li {
            if eligible_url(&file.download_url) {
                a { href: file.download_url.clone(), target: "_blank", rel: "noopener noreferrer",
                    onclick: move |event| {
                        if !activation.write().begin(started.elapsed()) { event.prevent_default(); }
                    },
                    "{file.filename}"
                }
            } else { span { "{file.filename} · File link unavailable" } }
            span { " · {file.content_type}" }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/files.rs"]
mod tests;
