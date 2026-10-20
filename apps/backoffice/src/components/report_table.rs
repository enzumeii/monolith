use dioxus::prelude::*;

use super::ErrorMessage;
use crate::pages::reports::{ExportFormat, download, encode_table, encode_table_xlsx};

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
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        div { class: "order-download",
            button { id: "{id}", class: "ghost", disabled: busy() || table.rows.is_empty(),
                onclick: {
                    let table = table.clone();
                    let filename = filename.clone();
                    let sheet_name = sheet_name.clone();
                    let id = id.clone();
                    move |_| {
                        if busy() || table.rows.is_empty() { return; }
                        busy.set(true);
                        error.set(None);
                        let table = table.clone();
                        let filename = filename.clone();
                        let sheet_name = sheet_name.clone();
                        let id = id.clone();
                        spawn(async move {
                            let bytes = match format {
                                ExportFormat::Csv => Ok(encode_table(table.headers, &table.rows).into_bytes()),
                                ExportFormat::Xlsx => encode_table_xlsx(table.headers, &table.rows, &sheet_name),
                            };
                            let result = match bytes {
                                Ok(bytes) => download(bytes, &filename, format, &id).await,
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
