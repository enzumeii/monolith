use dioxus::prelude::*;

use super::ErrorMessage;
use crate::export::{ExportFormat, download, encode_table, encode_table_xlsx};

#[derive(Clone, PartialEq)]
pub struct TableData {
    pub headers: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}

#[component]
pub fn DataTable(
    table: TableData,
    children: Option<Element>,
    #[props(default = "Order report table".to_string())] label: String,
) -> Element {
    rsx! {
        div { class: "report-scroll", tabindex: "0", role: "region", aria_label: "{label}",
            table { class: "admins-table report-table",
                thead { tr { for header in table.headers { th { "{header}" } } } }
                if let Some(children) = children {
                    {children}
                } else {
                    tbody {
                        for row in &table.rows {
                            tr { for value in row { td { "{value}" } } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn DownloadTable(
    table: TableData,
    filename: String,
    sheet_name: String,
    format: ExportFormat,
    id: String,
    label: String,
) -> Element {
    let disabled = table.rows.is_empty();
    rsx! {
        DownloadFile {
            filename, mime: format.mime(), id, label, disabled,
            encode: move |()| match format {
                ExportFormat::Csv => Ok(encode_table(table.headers, &table.rows).into_bytes()),
                ExportFormat::Xlsx => encode_table_xlsx(table.headers, &table.rows, &sheet_name),
            },
        }
    }
}

#[component]
pub fn DownloadFile(
    filename: String,
    mime: String,
    id: String,
    label: String,
    disabled: bool,
    encode: Callback<(), Result<Vec<u8>, String>>,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        div { class: "order-download",
            button { id: "{id}", class: "ghost", disabled: busy() || disabled,
                onclick: {
                    let filename = filename.clone();
                    let mime = mime.clone();
                    let id = id.clone();
                    move |_| {
                        if busy() || disabled { return; }
                        busy.set(true);
                        error.set(None);
                        let filename = filename.clone();
                        let mime = mime.clone();
                        let id = id.clone();
                        spawn(async move {
                            let result = match encode.call(()) {
                                Ok(bytes) => download(bytes, &filename, &mime, &id).await,
                                Err(message) => Err(message),
                            };
                            busy.set(false);
                            if let Err(message) = result { error.set(Some(message)); }
                        });
                    }
                },
                if busy() { "Saving…" } else { "{label}" }
            }
            ErrorMessage { message: error() }
        }
    }
}
