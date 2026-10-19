//! Wire-level tests for [`HttpCellarSink`] (Issue #191).
//!
//! Every request runs against an in-process mock cellar; nothing here touches an
//! external network. The point of these tests is the *shape* of what banquette
//! sends: ownership travels in the `X-Alhalte-Owner` / `X-Alhalte-Session-Id`
//! headers (or the service token), never in the JSON body.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use alhalte_banquette::cellar_sink::INTERNAL_WRITE_SESSION_ID;
use alhalte_banquette::{CellarSink, HttpCellarSink};
use alhalte_cellar::SYSTEM_OWNER;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

const SERVICE_TOKEN: &str = "test-service-token";

/// One request observed by the mock cellar.
#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Parses the JSON body, failing the test when it is not JSON.
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("{} body is not JSON: {e}", self.path))
    }
}

/// Shared state of the mock cellar.
#[derive(Clone)]
struct MockCellar {
    calls: Arc<Mutex<Vec<Recorded>>>,
    store_result: Arc<Mutex<&'static str>>,
}

impl MockCellar {
    fn new(store_result: &'static str) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            store_result: Arc::new(Mutex::new(store_result)),
        }
    }

    fn calls(&self) -> Vec<Recorded> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn record(&self, method: &str, path: String, headers: &HeaderMap, body: &[u8]) {
        let mut recorded_headers = HashMap::new();
        for (name, value) in headers.iter() {
            if let Ok(value) = value.to_str() {
                recorded_headers.insert(name.as_str().to_ascii_lowercase(), value.to_string());
            }
        }
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(Recorded {
                method: method.to_string(),
                path,
                headers: recorded_headers,
                body: body.to_vec(),
            });
        }
    }

    fn result(&self) -> &'static str {
        match self.store_result.lock() {
            Ok(result) => *result,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }
}

async fn store_check(
    State(cellar): State<MockCellar>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    cellar.record("POST", "/store".to_string(), &headers, &body);
    let result = cellar.result();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({ "result": result })),
    )
        .into_response()
}

async fn store_upload(
    State(cellar): State<MockCellar>,
    Path(tin_id): Path<String>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    cellar.record(method.as_str(), format!("/store/{tin_id}"), &headers, &body);
    // Echo the reference label the request claimed, like the real cellar does
    // for an authorized write.
    let owner = headers
        .get("x-alhalte-owner")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(SYSTEM_OWNER);
    let body = serde_json::json!({
        "tin_id": tin_id,
        "owner": owner,
        "written_to_disk": true,
        "reference_added": true,
    });
    (axum::http::StatusCode::CREATED, Json(body)).into_response()
}

async fn unregister(State(cellar): State<MockCellar>, headers: HeaderMap, body: Bytes) -> Response {
    cellar.record("POST", "/unregister".to_string(), &headers, &body);
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({ "unregistered": true })),
    )
        .into_response()
}

/// Spawns the mock cellar on an ephemeral loopback port.
async fn spawn_mock_cellar(cellar: MockCellar) -> String {
    let app = Router::new()
        .route("/store", post(store_check))
        .route("/store/{tin_id}", post(store_upload).put(store_upload))
        .route("/unregister", post(unregister))
        .route("/health", get(|| async { "ok" }))
        .with_state(cellar);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn tin_bytes() -> ([u8; 32], Vec<u8>) {
    ([0x11u8; 32], b"room-state-wire-bytes".to_vec())
}

// spec: alhalte-banquette/persistence 0010 store_tin sends identity in headers only (Issue #191)
#[tokio::test]
async fn test_store_tin_sends_owner_and_session_as_headers() {
    let mock = MockCellar::new("upload_required");
    let base_url = spawn_mock_cellar(mock.clone()).await;
    let sink = HttpCellarSink::new(base_url);

    let (tin_id, bytes) = tin_bytes();
    let wrapped_key = vec![0xAAu8, 0xBB];
    let result = sink
        .store_tin(42, 7, &bytes, &tin_id, Some(wrapped_key.clone()))
        .await
        .unwrap();
    assert_eq!(result.owner, 42);

    let calls = mock.calls();
    assert_eq!(calls.len(), 2, "handshake then upload: {calls:?}");

    // 1. Handshake: metadata body, identity in headers.
    let handshake = &calls[0];
    assert_eq!(handshake.method, "POST");
    assert_eq!(handshake.path, "/store");
    assert_eq!(handshake.header("x-alhalte-owner"), Some("42"));
    assert_eq!(handshake.header("x-alhalte-session-id"), Some("7"));
    assert_eq!(handshake.header("x-alhalte-service-token"), None);
    let body = handshake.json();
    assert_eq!(body["tin_id"], hex::encode(tin_id));
    assert_eq!(body["size"], bytes.len() as u64);
    assert_eq!(body["wrapped_key"], hex::encode(&wrapped_key));
    assert!(body.get("owner").is_none(), "owner must not be in the body");
    assert!(
        body.get("session_id").is_none(),
        "session_id must not be in the body"
    );

    // 2. Upload: same headers, body is the raw tin.
    let upload = &calls[1];
    assert_eq!(upload.method, "POST");
    assert_eq!(upload.path, format!("/store/{}", hex::encode(tin_id)));
    assert_eq!(upload.header("x-alhalte-owner"), Some("42"));
    assert_eq!(upload.header("x-alhalte-session-id"), Some("7"));
    assert_eq!(upload.header("x-alhalte-wrapped-key"), Some("aabb"));
    assert_eq!(upload.body, bytes);
}

// spec: alhalte-banquette/persistence 0011 service-token writes are labelled SYSTEM_OWNER (Issue #191)
#[tokio::test]
async fn test_store_tin_with_service_token_uses_the_system_owner() {
    // Deduplicated keeps the handshake-only path, where the sink itself reports
    // the label rather than the cellar's `StoreResult`.
    let mock = MockCellar::new("deduplicated");
    let base_url = spawn_mock_cellar(mock.clone()).await;
    let sink = HttpCellarSink::new(base_url).with_service_token(SERVICE_TOKEN);

    let (tin_id, bytes) = tin_bytes();
    let result = sink
        .store_tin(42, INTERNAL_WRITE_SESSION_ID, &bytes, &tin_id, None)
        .await
        .unwrap();
    assert_eq!(
        result.owner, SYSTEM_OWNER,
        "a service-token write must report the system owner label"
    );
    assert!(!result.written_to_disk);
    assert!(result.reference_added);

    let calls = mock.calls();
    assert_eq!(calls.len(), 1);
    let handshake = &calls[0];
    assert_eq!(
        handshake.header("x-alhalte-owner"),
        Some(SYSTEM_OWNER.to_string().as_str()),
        "the room administrator must not label an internal write"
    );
    assert_eq!(
        handshake.header("x-alhalte-service-token"),
        Some(SERVICE_TOKEN)
    );
    let body = handshake.json();
    assert!(body.get("owner").is_none());
    assert!(body.get("session_id").is_none());
}

// spec: alhalte-banquette/persistence 0012 unregister_tin sends the tin id in the body only (Issue #191)
#[tokio::test]
async fn test_unregister_tin_sends_headers_and_a_minimal_body() {
    let mock = MockCellar::new("deduplicated");
    let base_url = spawn_mock_cellar(mock.clone()).await;
    let sink = HttpCellarSink::new(base_url);

    let (tin_id, _bytes) = tin_bytes();
    assert!(
        sink.unregister_tin(42, 7, &tin_id).await.unwrap(),
        "a 200 answer removes the reference"
    );

    let calls = mock.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.path, "/unregister");
    assert_eq!(call.header("x-alhalte-owner"), Some("42"));
    assert_eq!(call.header("x-alhalte-session-id"), Some("7"));
    let body = call.json();
    assert_eq!(body["tin_id"], hex::encode(tin_id));
    assert!(body.get("owner").is_none());
    assert!(body.get("session_id").is_none());
}
