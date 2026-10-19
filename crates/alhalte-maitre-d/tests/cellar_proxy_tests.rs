//! Tests for the authenticated `/api/cellar/*` reverse proxy (Issue #109 / #138).
//!
//! Every test runs against an in-process mock `alhalte-cellar`; nothing here
//! touches an external network.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, RawQuery, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use alhalte_maitre_d::server::CellarProxy;
use alhalte_maitre_d::{
    AppState, Database, MockOAuthProvider, RoomBridgeManager, SubHash, create_router, hash_sub,
};

/// Hashes a test `sub` the way the OAuth path does (Issue #190).
fn test_sub_hash(sub: &str) -> SubHash {
    let mut sub = sub.to_string();
    hash_sub("test-salt", &mut sub)
}

const TIN_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const WRAPPED_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// The mock cellar mirrors the real service's 64 MiB upload limit; without it
/// the mock's own Axum 2 MiB default would reject the >2 MiB regression body.
const MOCK_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// One request observed by the mock cellar.
#[derive(Debug, Clone)]
struct RecordedCall {
    method: String,
    path: String,
    query: Option<String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl RecordedCall {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// Shared state of the in-process mock cellar.
struct MockState {
    calls: Vec<RecordedCall>,
    store_result: String,
    upload_status: u16,
    download_status: StatusCode,
    download_body: Vec<u8>,
    wrapped_key: Option<String>,
    delay: Option<Duration>,
}

/// In-process mock `alhalte-cellar` that records what the proxy sent.
#[derive(Clone)]
struct MockCellar {
    inner: Arc<Mutex<MockState>>,
}

impl MockCellar {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockState {
                calls: Vec::new(),
                store_result: "upload_required".to_string(),
                upload_status: StatusCode::CREATED.as_u16(),
                download_status: StatusCode::OK,
                download_body: b"wire-bytes".to_vec(),
                wrapped_key: Some(WRAPPED_KEY.to_string()),
                delay: None,
            })),
        }
    }

    fn set_store_result(&self, result: &str) {
        if let Ok(mut state) = self.inner.lock() {
            state.store_result = result.to_string();
        }
    }

    fn set_delay(&self, delay: Duration) {
        if let Ok(mut state) = self.inner.lock() {
            state.delay = Some(delay);
        }
    }

    fn calls(&self) -> Vec<RecordedCall> {
        match self.inner.lock() {
            Ok(state) => state.calls.clone(),
            Err(poisoned) => poisoned.into_inner().calls.clone(),
        }
    }

    fn record(
        &self,
        method: &str,
        path: String,
        headers: &HeaderMap,
        query: Option<String>,
        body: &[u8],
    ) {
        let mut recorded_headers = HashMap::new();
        for (name, value) in headers.iter() {
            if let Ok(value) = value.to_str() {
                recorded_headers.insert(name.as_str().to_ascii_lowercase(), value.to_string());
            }
        }
        if let Ok(mut state) = self.inner.lock() {
            state.calls.push(RecordedCall {
                method: method.to_string(),
                path,
                query,
                headers: recorded_headers,
                body: body.to_vec(),
            });
        }
    }

    async fn maybe_delay(&self) {
        let delay = match self.inner.lock() {
            Ok(state) => state.delay,
            Err(poisoned) => poisoned.into_inner().delay,
        };
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
    }
}

/// Builds the mock cellar router.
fn mock_cellar_router(cellar: MockCellar) -> Router {
    Router::new()
        .route("/store", post(mock_store_check))
        .route(
            "/store/{tin_id}",
            post(mock_store_upload)
                .put(mock_store_upload)
                .layer(DefaultBodyLimit::max(MOCK_MAX_BODY_BYTES)),
        )
        .route("/tin/{tin_id}", get(mock_download))
        .with_state(cellar)
}

async fn mock_store_check(
    State(cellar): State<MockCellar>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    cellar.maybe_delay().await;
    cellar.record("POST", "/store".to_string(), &headers, query, &body);
    let result = match cellar.inner.lock() {
        Ok(state) => state.store_result.clone(),
        Err(poisoned) => poisoned.into_inner().store_result.clone(),
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "result": result })),
    )
        .into_response()
}

async fn mock_store_upload(
    State(cellar): State<MockCellar>,
    Path(tin_id): Path<String>,
    method: Method,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    cellar.maybe_delay().await;
    let path = format!("/store/{tin_id}");
    cellar.record(method.as_str(), path, &headers, query, &body);
    let status = match cellar.inner.lock() {
        Ok(state) => state.upload_status,
        Err(poisoned) => poisoned.into_inner().upload_status,
    };
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::CREATED),
        Json(serde_json::json!({ "tin_id": tin_id, "uploaded": true })),
    )
        .into_response()
}

async fn mock_download(
    State(cellar): State<MockCellar>,
    Path(tin_id): Path<String>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    cellar.maybe_delay().await;
    cellar.record("GET", format!("/tin/{tin_id}"), &headers, query, &[]);
    let Ok(state) = cellar.inner.lock() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/octet-stream"),
    );
    if let Some(key) = state.wrapped_key.as_ref()
        && let Ok(value) = axum::http::HeaderValue::from_str(key)
    {
        response_headers.insert("X-Alhalte-Wrapped-Key", value);
    }
    (
        state.download_status,
        response_headers,
        state.download_body.clone(),
    )
        .into_response()
}

/// Spawns the mock cellar on an ephemeral port.
async fn spawn_mock_cellar(cellar: MockCellar) -> SocketAddr {
    let app = mock_cellar_router(cellar);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

/// Creates an in-memory maitre-d database with one account and one session.
fn test_identity() -> (Database, u64, u64) {
    let db = Database::new_in_memory().unwrap();
    let account = db
        .get_or_create_account(
            &test_sub_hash("sub-cellar-proxy"),
            "Proxy Tester",
            None,
            1000,
        )
        .unwrap();
    let session = db.create_session(account.account_id, 1000).unwrap();
    (db, account.account_id, session.session_id)
}

/// Builds a maitre-d [`AppState`] without a cellar proxy.
fn base_app_state(db: Database) -> AppState {
    let spawner =
        Arc::new(alhalte_maitre_d::InProcessBanquetteSpawner::new_with_in_memory_cellar("/tmp", 8));
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    AppState::new(db, "salt", oauth, room_manager)
}

/// Spawns maitre-d with an optional cellar proxy and returns its address.
async fn spawn_maitre_d(db: Database, cellar: Option<CellarProxy>) -> SocketAddr {
    let state = base_app_state(db);
    let state = match cellar {
        Some(cellar) => state.with_cellar_proxy(cellar),
        None => state,
    };

    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

fn cookie_header(session_id: u64) -> String {
    format!("session_id={session_id}")
}

// spec: alhalte-maitre-d/cellar-proxy store check 200 + result (headers only, Issue #191)
#[tokio::test]
async fn test_cellar_proxy_store_check_injects_identity_headers() {
    let mock = MockCellar::new();
    mock.set_store_result("upload_required");
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;

    // The body carries metadata only; ownership comes from the session.
    let body = serde_json::json!({
        "tin_id": TIN_ID,
        "size": 1234u64,
        "wrapped_key": null,
    });
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/cellar/store"))
        .header("Cookie", cookie_header(session_id))
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(&body).unwrap())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = response.json().await.unwrap();
    assert_eq!(payload, serde_json::json!({ "result": "upload_required" }));

    let calls = mock.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.path, "/store");
    assert_eq!(
        call.header("x-alhalte-owner"),
        Some(account_id.to_string().as_str())
    );
    assert_eq!(
        call.header("x-alhalte-session-id"),
        Some(session_id.to_string().as_str())
    );

    // The body is forwarded verbatim: cellar sees metadata only, no ownership.
    let forwarded: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(forwarded["tin_id"], TIN_ID);
    assert_eq!(forwarded["size"], 1234);
    assert!(forwarded.get("owner").is_none());
    assert!(forwarded.get("session_id").is_none());
}

// spec: alhalte-maitre-d/cellar-proxy rejects a body that claims owner / session_id (Issue #191)
#[tokio::test]
async fn test_cellar_proxy_rejects_owner_claims_in_the_store_body() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;
    let client = reqwest::Client::new();

    // The pre-#191 contract (owner / session_id in the body) fails loudly
    // instead of having the claim silently rewritten or ignored.
    for body in [
        serde_json::json!({
            "owner": account_id,
            "session_id": session_id,
            "tin_id": TIN_ID,
            "size": 1u64,
        }),
        serde_json::json!({
            "owner": account_id + 1,
            "tin_id": TIN_ID,
            "size": 1u64,
        }),
        serde_json::json!({
            "session_id": session_id,
            "tin_id": TIN_ID,
            "size": 1u64,
        }),
    ] {
        let response = client
            .post(format!("http://{addr}/api/cellar/store"))
            .header("Cookie", cookie_header(session_id))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "body {body} must be rejected"
        );
    }

    // Nothing reached cellar.
    assert!(mock.calls().is_empty());
}

// spec: alhalte-maitre-d/cellar-proxy deduplicated/already_owned pass through without a PUT
#[tokio::test]
async fn test_cellar_proxy_store_check_passes_results_through() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, _account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;

    let client = reqwest::Client::new();
    for result in ["deduplicated", "already_owned"] {
        mock.set_store_result(result);
        let response = client
            .post(format!("http://{addr}/api/cellar/store"))
            .header("Cookie", cookie_header(session_id))
            .json(&serde_json::json!({
                "tin_id": TIN_ID,
                "size": 1u64,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let payload: serde_json::Value = response.json().await.unwrap();
        assert_eq!(payload, serde_json::json!({ "result": result }));
    }

    // No upload followed the handshakes.
    assert!(mock.calls().iter().all(|call| call.path == "/store"));
}

// spec: alhalte-maitre-d/cellar-proxy owner spoofing is rejected
#[tokio::test]
async fn test_cellar_proxy_rejects_spoofed_owner() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;
    let client = reqwest::Client::new();
    let other_account = account_id + 1;

    // 1. Spoofed owner via header.
    let response = client
        .get(format!("http://{addr}/api/cellar/tin/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .header("X-Alhalte-Account", other_account.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

    // 2. Spoofed owner via query parameter.
    let response = client
        .get(format!(
            "http://{addr}/api/cellar/tin/{TIN_ID}?owner={other_account}&session_id={session_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

    // 3. A spoofed owner claim is refused. Ownership is never part of the body
    //    (a body claim is 400, see the dedicated test) and a query claim that
    //    contradicts the session is 403.
    let response = client
        .post(format!(
            "http://{addr}/api/cellar/store?account_id={other_account}"
        ))
        .header("Cookie", cookie_header(session_id))
        .json(&serde_json::json!({ "tin_id": TIN_ID, "size": 1u64 }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

    // The mock cellar never saw any of the rejected requests.
    assert!(mock.calls().is_empty());
}

// spec: alhalte-maitre-d/cellar-proxy requires an authenticated session
#[tokio::test]
async fn test_cellar_proxy_requires_valid_session() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, _account_id, _session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;
    let client = reqwest::Client::new();

    // No session at all.
    let response = client
        .get(format!("http://{addr}/api/cellar/tin/{TIN_ID}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Unknown session id.
    let response = client
        .get(format!("http://{addr}/api/cellar/tin/{TIN_ID}"))
        .header("Cookie", cookie_header(999_999))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    assert!(mock.calls().is_empty());
}

// spec: alhalte-maitre-d/cellar-proxy upload forwards body and wrapped key
#[tokio::test]
async fn test_cellar_proxy_upload_forwards_body_and_wrapped_key() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;
    let client = reqwest::Client::new();

    let wire_bytes = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];

    let response = client
        .post(format!("http://{addr}/api/cellar/store/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .header("X-Alhalte-Wrapped-Key", WRAPPED_KEY)
        .header("Content-Type", "application/octet-stream")
        .body(wire_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let payload: serde_json::Value = response.json().await.unwrap();
    assert_eq!(payload["tin_id"], TIN_ID);

    let calls = mock.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.path, format!("/store/{TIN_ID}"));
    assert_eq!(call.body, wire_bytes);
    assert_eq!(call.header("x-alhalte-wrapped-key"), Some(WRAPPED_KEY));
    assert_eq!(
        call.header("x-alhalte-owner"),
        Some(account_id.to_string().as_str())
    );
    assert_eq!(
        call.header("x-alhalte-session-id"),
        Some(session_id.to_string().as_str())
    );

    // PUT must be forwarded as PUT, not coerced to POST.
    let response = client
        .put(format!("http://{addr}/api/cellar/store/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .body(wire_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let calls = mock.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].method, "PUT");
    assert_eq!(calls[1].body, wire_bytes);
}

// spec: alhalte-maitre-d/cellar-proxy raises the store body limit above Axum's 2 MiB default (Issue #109 / #146)
#[tokio::test]
async fn test_cellar_proxy_accepts_body_larger_than_default_limit() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, _account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;
    let client = reqwest::Client::new();

    // Axum's default request body limit is 2 MiB. A 3 MiB + 1 tin must reach
    // the cellar intact through the proxy.
    let wire_bytes = vec![0xA5u8; 3 * 1024 * 1024 + 1];

    let response = client
        .post(format!("http://{addr}/api/cellar/store/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .header("Content-Type", "application/octet-stream")
        .body(wire_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let calls = mock.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].path, format!("/store/{TIN_ID}"));
    assert_eq!(calls[0].body.len(), wire_bytes.len());
    assert_eq!(calls[0].body, wire_bytes);
}

// spec: alhalte-maitre-d/cellar-proxy rejects malformed tin ids before forwarding
#[tokio::test]
async fn test_cellar_proxy_rejects_invalid_tin_id() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, _account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;
    let client = reqwest::Client::new();

    for bad in ["notahex", "abc", &"f".repeat(63), &"g".repeat(64)] {
        let response = client
            .get(format!("http://{addr}/api/cellar/tin/{bad}"))
            .header("Cookie", cookie_header(session_id))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "tin id {bad:?} should be rejected"
        );
    }

    // A traversal attempt cannot even match the single-segment route, so it is
    // rejected by the router before the handler runs.
    let response = client
        .get(format!("http://{addr}/api/cellar/tin/../../etc/passwd"))
        .header("Cookie", cookie_header(session_id))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);

    assert!(mock.calls().is_empty());
}

// spec: alhalte-maitre-d/cellar-proxy download forwards bytes and wrapped key
#[tokio::test]
async fn test_cellar_proxy_download_forwards_bytes_and_wrapped_key() {
    let mock = MockCellar::new();
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;

    let response = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/cellar/tin/{TIN_ID}?owner={account_id}&session_id={session_id}"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let wrapped_key = response.headers().get("X-Alhalte-Wrapped-Key").cloned();
    assert_eq!(
        wrapped_key.map(|v| v.to_str().unwrap().to_string()),
        Some(WRAPPED_KEY.to_string())
    );
    let body = response.bytes().await.unwrap();
    assert_eq!(body.as_ref(), b"wire-bytes");

    let calls = mock.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.method, "GET");
    assert_eq!(call.path, format!("/tin/{TIN_ID}"));
    // The authenticated owner is injected as a header; the client's query
    // string (including `owner` / `session_id`) must not reach cellar.
    assert_eq!(
        call.header("x-alhalte-owner"),
        Some(account_id.to_string().as_str())
    );
    assert_eq!(call.query, None);
}

// spec: alhalte-maitre-d/cellar-proxy maps upstream failures to 502
#[tokio::test]
async fn test_cellar_proxy_maps_upstream_failure_to_502() {
    // Reserve a port and drop the listener so connections are refused.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = listener.local_addr().unwrap();
    drop(listener);

    let (db, _account_id, session_id) = test_identity();
    let proxy = CellarProxy::new(format!("http://{dead_addr}")).unwrap();
    let addr = spawn_maitre_d(db, Some(proxy)).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/cellar/tin/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let body = response.text().await.unwrap();
    // Never leak the internal cellar address.
    assert!(
        !body.contains("127.0.0.1"),
        "response leaked upstream host: {body}"
    );
    assert!(!body.contains(&dead_addr.port().to_string()));
}

// spec: alhalte-maitre-d/cellar-proxy maps upstream timeouts to 504
#[tokio::test]
async fn test_cellar_proxy_maps_upstream_timeout_to_504() {
    let mock = MockCellar::new();
    mock.set_delay(Duration::from_millis(600));
    let cellar_addr = spawn_mock_cellar(mock.clone()).await;

    let (db, _account_id, session_id) = test_identity();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let proxy = CellarProxy::new(format!("http://{cellar_addr}"))
        .unwrap()
        .with_client(client);
    let addr = spawn_maitre_d(db, Some(proxy)).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/cellar/tin/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::GATEWAY_TIMEOUT);
    let body = response.text().await.unwrap();
    assert!(
        !body.contains("127.0.0.1"),
        "response leaked upstream host: {body}"
    );
}

// spec: alhalte-maitre-d/cellar-proxy unconfigured proxy answers 503
#[tokio::test]
async fn test_cellar_proxy_unconfigured_returns_503() {
    let (db, _account_id, session_id) = test_identity();
    let addr = spawn_maitre_d(db, None).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/api/cellar/tin/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
}

// spec: alhalte-maitre-d/cellar-proxy base URL validation
#[test]
fn test_cellar_proxy_base_url_validation() {
    assert!(CellarProxy::new("not-a-url").is_err());
    assert!(CellarProxy::new("ftp://cellar.example").is_err());
    assert!(CellarProxy::new("").is_err());

    let proxy = CellarProxy::new("http://127.0.0.1:8081/").unwrap();
    assert_eq!(proxy.base_url(), "http://127.0.0.1:8081");
    let proxy = CellarProxy::new("https://cellar.example/prefix").unwrap();
    assert_eq!(proxy.base_url(), "https://cellar.example/prefix");
}

// spec: alhalte-maitre-d/cellar-proxy --cellar-url plumbing into AppState
#[test]
fn test_app_state_with_cellar_url_plumbing() {
    let (db, _account_id, _session_id) = test_identity();
    let state = base_app_state(db)
        .with_cellar_url("http://127.0.0.1:9/")
        .unwrap();
    assert_eq!(
        state.cellar.as_ref().map(CellarProxy::base_url),
        Some("http://127.0.0.1:9")
    );

    let (db, _account_id, _session_id) = test_identity();
    assert!(base_app_state(db).with_cellar_url("nope").is_err());
}

// spec: alhalte-maitre-d/cellar-proxy end-to-end against a real alhalte-cellar
#[tokio::test]
async fn test_cellar_proxy_end_to_end_with_real_cellar() {
    use alhalte_cellar::{CellarService, MaitreDVerifier};

    let temp = tempfile::tempdir().unwrap();
    let (db, account_id, session_id) = test_identity();

    // Both addresses must be known before either router is built: the proxy
    // needs the cellar URL and the cellar verifies sessions through maitre-d.
    let maitre_d_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let maitre_d_addr = maitre_d_listener.local_addr().unwrap();
    let cellar_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cellar_addr = cellar_listener.local_addr().unwrap();

    let verifier = Arc::new(MaitreDVerifier::new_http(format!("http://{maitre_d_addr}")));
    let service =
        Arc::new(CellarService::new_with_in_memory_db(temp.path(), verifier, false).unwrap());
    let cellar_app = alhalte_cellar::create_router(service);
    tokio::spawn(async move {
        let _ = axum::serve(cellar_listener, cellar_app).await;
    });

    let state = base_app_state(db)
        .with_cellar_url(format!("http://{cellar_addr}"))
        .unwrap();
    let app = create_router(state);
    tokio::spawn(async move {
        let _ = axum::serve(maitre_d_listener, app).await;
    });

    let client = reqwest::Client::new();
    let base = format!("http://{maitre_d_addr}/api/cellar");
    let wire_bytes = b"room-asset-wire-bytes".to_vec();
    let wrapped_key = "aabbccdd";

    // 1. First handshake: the cellar has never seen this tin -> upload_required.
    //    The body carries metadata only; the proxy supplies the identity from
    //    the session headers (Issue #191).
    let response = client
        .post(format!("{base}/store"))
        .header("Cookie", cookie_header(session_id))
        .json(&serde_json::json!({
            "tin_id": TIN_ID,
            "size": wire_bytes.len(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = response.json().await.unwrap();
    assert_eq!(payload, serde_json::json!({ "result": "upload_required" }));

    // 1b. An ownership claim in the body is refused by the proxy before it can
    //     reach the cellar.
    let response = client
        .post(format!("{base}/store"))
        .header("Cookie", cookie_header(session_id))
        .json(&serde_json::json!({
            "owner": account_id + 1,
            "session_id": session_id + 1,
            "tin_id": TIN_ID,
            "size": wire_bytes.len(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    // 2. Upload the body.
    let response = client
        .put(format!("{base}/store/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .header("X-Alhalte-Wrapped-Key", wrapped_key)
        .body(wire_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let payload: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        payload["owner"], account_id,
        "the stored reference belongs to the session owner"
    );

    // 3. A second handshake now reports deduplicated or already_owned.
    let response = client
        .post(format!("{base}/store"))
        .header("Cookie", cookie_header(session_id))
        .json(&serde_json::json!({
            "tin_id": TIN_ID,
            "size": wire_bytes.len(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = response.json().await.unwrap();
    let result = payload["result"].as_str().unwrap_or_default();
    assert!(
        result == "deduplicated" || result == "already_owned",
        "unexpected handshake result {result}"
    );

    // 4. Download through the proxy: bytes and wrapped key round-trip.
    let response = client
        .get(format!("{base}/tin/{TIN_ID}"))
        .header("Cookie", cookie_header(session_id))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("X-Alhalte-Wrapped-Key")
            .and_then(|v| v.to_str().ok()),
        Some(wrapped_key)
    );
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        wire_bytes.as_slice()
    );

    // 5. Sessions are still enforced end to end.
    let response = client
        .get(format!("{base}/tin/{TIN_ID}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}
