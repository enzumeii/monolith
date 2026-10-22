#![allow(clippy::unwrap_used)]
use crate::{client::vercel_blob::VercelBlobClient, config::VercelBlobConfig};
use axum::{
    Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub(crate) struct Data {
    pub objects: HashMap<String, Vec<u8>>,
    pub puts: Vec<String>,
    pub fail_key: Option<String>,
    pub lost_response: bool,
    pub malformed: bool,
    pub failure: Option<(&'static str, &'static str)>,
    pub calls: Vec<String>,
}
#[derive(Clone)]
struct Shared {
    base: String,
    data: Arc<Mutex<Data>>,
}
pub(crate) struct Fixture {
    pub base: String,
    pub data: Arc<Mutex<Data>>,
    #[allow(
        dead_code,
        reason = "TCP tests exercise the running application client instead"
    )]
    pub client: VercelBlobClient,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    pub async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let data = Arc::new(Mutex::new(Data::default()));
        let app = Router::new()
            .route("/", get(metadata).put(upload))
            .route("/{*key}", get(download))
            .with_state(Shared {
                base: base.clone(),
                data: data.clone(),
            });
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = VercelBlobClient::new(&VercelBlobConfig {
            api_url: base.clone(),
            timeout_secs: 1,
            token: Some("synthetic-blob-token".into()),
        })
        .unwrap();
        Self {
            base,
            data,
            client,
            task,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn blob(shared: &Shared, key: &str) -> serde_json::Value {
    json!({"pathname":key,"contentType":"text/csv; charset=utf-8","url":format!("{}/{key}",shared.base),"downloadUrl":format!("{}/{key}?download=1",shared.base)})
}
async fn metadata(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> axum::response::Response {
    if let Some(response) = failure(&shared, "metadata").await {
        return response;
    }
    assert_eq!(headers["authorization"], "Bearer synthetic-blob-token");
    assert_eq!(headers["x-api-version"], "13");
    let key = &query["url"];
    if shared.data.lock().unwrap().objects.contains_key(key) {
        axum::Json(blob(&shared, key)).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
async fn upload(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    bytes: Bytes,
) -> axum::response::Response {
    if let Some(response) = failure(&shared, "upload").await {
        return response;
    }
    assert_eq!(headers["authorization"], "Bearer synthetic-blob-token");
    assert_eq!(headers["x-api-version"], "13");
    assert_eq!(headers["x-vercel-blob-access"], "public");
    assert_eq!(headers["x-add-random-suffix"], "0");
    assert_eq!(headers["x-allow-overwrite"], "0");
    assert_eq!(headers["x-content-type"], "text/csv; charset=utf-8");
    let key = &query["pathname"];
    let (lost, malformed) = {
        let mut data = shared.data.lock().unwrap();
        data.puts.push(key.clone());
        if data.fail_key.as_ref() == Some(key) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if data.objects.contains_key(key) {
            return StatusCode::PRECONDITION_FAILED.into_response();
        }
        data.objects.insert(key.clone(), bytes.to_vec());
        (data.lost_response, data.malformed)
    };
    if lost {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    if malformed {
        return "invalid json".into_response();
    }
    axum::Json(blob(&shared, key)).into_response()
}
async fn download(
    State(shared): State<Shared>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Some(response) = failure(&shared, "download").await {
        return response;
    }
    assert!(!headers.contains_key("authorization"));
    match shared.data.lock().unwrap().objects.get(&key) {
        Some(bytes) => {
            ([("content-type", "text/csv; charset=utf-8")], bytes.clone()).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn failure(shared: &Shared, operation: &str) -> Option<axum::response::Response> {
    let fault = {
        let mut data = shared.data.lock().unwrap();
        data.calls.push(operation.to_owned());
        data.failure
    };
    let (stage, kind) = fault?;
    if stage == "recovery" {
        if operation == "upload" {
            return Some(StatusCode::SERVICE_UNAVAILABLE.into_response());
        }
        let uploaded = shared
            .data
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|call| call == "upload");
        if operation != "metadata" || !uploaded {
            return None;
        }
    } else if stage != operation {
        return None;
    }
    Some(match kind {
        "http" => (StatusCode::SERVICE_UNAVAILABLE, "private Blob failure").into_response(),
        "malformed" => "{".into_response(),
        "checksum" => "corrupt report bytes".into_response(),
        "timeout" => {
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        _ => panic!("unknown Blob fixture failure"),
    })
}
