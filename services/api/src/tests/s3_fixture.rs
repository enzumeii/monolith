#![allow(clippy::unwrap_used)]
use crate::{client::s3::S3Client, config::S3Config};
use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
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
    pub content_type: Option<String>,
    pub download_override: Option<Vec<u8>>,
    pub put_gate: Option<Arc<PutGate>>,
}
pub(crate) struct PutGate {
    pub started: tokio::sync::Semaphore,
    pub release: tokio::sync::Semaphore,
    pub accepted: tokio::sync::Semaphore,
}
impl Default for PutGate {
    fn default() -> Self {
        Self {
            started: tokio::sync::Semaphore::new(0),
            release: tokio::sync::Semaphore::new(0),
            accepted: tokio::sync::Semaphore::new(0),
        }
    }
}
pub(crate) struct Fixture {
    pub base: String,
    pub data: Arc<Mutex<Data>>,
    pub client: S3Client,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    pub async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let data = Arc::new(Mutex::new(Data::default()));
        let app = Router::new()
            .route("/reports/{*key}", get(download).head(metadata).put(upload))
            .with_state(data.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = S3Client::new(&Self::config(&base)).unwrap();
        Self {
            base,
            data,
            client,
            task,
        }
    }
    pub fn config(endpoint: &str) -> S3Config {
        S3Config {
            endpoint: endpoint.into(),
            region: "ap-southeast-1".into(),
            bucket: "reports".into(),
            force_path_style: true,
            timeout_secs: 1,
            signed_url_ttl_secs: 300,
            access_key_id: Some("synthetic-access-id".into()),
            secret_access_key: Some("synthetic-secret-key".into()),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn authenticated(headers: &HeaderMap) {
    let auth = headers["authorization"].to_str().unwrap();
    assert!(auth.starts_with("AWS4-HMAC-SHA256 "));
    assert!(auth.contains("Credential=synthetic-access-id/"));
    assert!(auth.contains("/ap-southeast-1/s3/aws4_request"));
    assert!(headers.contains_key("x-amz-date"));
    assert!(
        !headers.contains_key("if-none-match"),
        "S3 upserts are not conditional creates"
    );
}
async fn metadata(
    State(shared): State<Arc<Mutex<Data>>>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    authenticated(&headers);
    if let Some(response) = failure(&shared, "metadata").await {
        return response;
    }
    let data = shared.lock().unwrap();
    match data.objects.get(&key) {
        Some(bytes) => {
            let mut response = StatusCode::OK.into_response();
            response
                .headers_mut()
                .insert("content-length", bytes.len().to_string().parse().unwrap());
            response.headers_mut().insert(
                "content-type",
                data.content_type
                    .as_deref()
                    .unwrap_or("text/csv; charset=utf-8")
                    .parse()
                    .unwrap(),
            );
            response
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn upload(
    State(shared): State<Arc<Mutex<Data>>>,
    Path(key): Path<String>,
    headers: HeaderMap,
    bytes: Bytes,
) -> axum::response::Response {
    authenticated(&headers);
    assert_eq!(headers["content-type"], "text/csv; charset=utf-8");
    if let Some(response) = failure(&shared, "upload").await {
        return response;
    }
    let gate = shared.lock().unwrap().put_gate.clone();
    if let Some(gate) = &gate {
        gate.started.add_permits(1);
        gate.release.acquire().await.unwrap().forget();
    }
    let (lost, malformed) = {
        let mut data = shared.lock().unwrap();
        data.puts.push(key.clone());
        if data.fail_key.as_ref() == Some(&key) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        // Supabase-compatible PUT is an upsert. Publication must make replacements content-identical.
        data.objects.insert(key, bytes.to_vec());
        (data.lost_response, data.malformed)
    };
    if let Some(gate) = gate {
        gate.accepted.add_permits(1);
    }
    if lost {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    if malformed {
        return (
            StatusCode::BAD_GATEWAY,
            "<Error><Code>InternalError</Code></Error>",
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}
async fn download(
    State(shared): State<Arc<Mutex<Data>>>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    authenticated(&headers);
    if let Some(response) = failure(&shared, "download").await {
        return response;
    }
    let data = shared.lock().unwrap();
    match data.objects.get(&key) {
        Some(bytes) => (
            [("content-type", "text/csv; charset=utf-8")],
            data.download_override.as_ref().unwrap_or(bytes).clone(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn failure(shared: &Arc<Mutex<Data>>, operation: &str) -> Option<axum::response::Response> {
    let fault = {
        let mut data = shared.lock().unwrap();
        data.calls.push(operation.to_owned());
        data.failure
    };
    let (stage, kind) = fault?;
    if stage == "recovery" {
        if operation == "upload" {
            return Some(StatusCode::SERVICE_UNAVAILABLE.into_response());
        }
        let uploaded = shared
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
        "http" => (
            StatusCode::SERVICE_UNAVAILABLE,
            "<Error><Code>InternalError</Code><Message>private S3 failure</Message></Error>",
        )
            .into_response(),
        "malformed" if operation == "metadata" => StatusCode::OK.into_response(),
        "malformed" => (StatusCode::OK, "corrupt metadata or report bytes").into_response(),
        "checksum" => "corrupt report bytes".into_response(),
        "timeout" => {
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        _ => panic!("unknown S3 fixture failure"),
    })
}
