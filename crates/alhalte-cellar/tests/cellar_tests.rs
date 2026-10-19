//! Integration tests for `alhalte-cellar`.

use std::sync::Arc;

use alhalte_cellar::CellarError;
use alhalte_cellar::CellarService;
use alhalte_cellar::db::Database;
use alhalte_cellar::model::{StoreRequest, StoreStatus, UnregisterRequest, format_tin_id};
use alhalte_cellar::storage::StorageManager;
use alhalte_cellar::verifier::{
    InMemorySessionVerifier, MaitreDVerifier, SessionVerifier, VerifierMode,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tin::Tin;
use tower::ServiceExt;

/// Helper to generate a dummy tin and its tin_id using `tin` crate.
fn create_test_tin(text: &str) -> (Vec<u8>, tin::TinId) {
    let t = Tin::from_string(text);
    t.encode_plaintext(tin::Compression::Zstd).unwrap()
}

// spec: alhalte-cellar/model
#[tokio::test]
async fn test_sqlite_and_file_entity_model() {
    let dir = tempdir().unwrap();
    let storage_dir = dir.path().join("tin");
    let db_path = dir.path().join("cellar.sqlite3");

    let storage = StorageManager::new(&storage_dir).unwrap();
    let conn = Database::open(&db_path).unwrap();

    let (tin_bytes, tin_id) = create_test_tin("test entity model");
    let relative_path = StorageManager::relative_name(&tin_id);
    let wrapped_key = Some(vec![1, 2, 3, 4, 5]);

    // 1. Write file to storage
    let saved_path = storage.write_tin(&tin_id, &tin_bytes).unwrap();
    assert!(saved_path.is_file());
    assert!(storage.has_tin_file(&tin_id));

    // 2. Insert reference to SQLite DB
    Database::insert_reference(&conn, &tin_id, &relative_path, 100, wrapped_key.as_deref())
        .unwrap();

    // 3. Verify reference query
    let reference = Database::get_reference(&conn, &tin_id, Some(100))
        .unwrap()
        .expect("reference row must exist");
    assert_eq!(reference.tin_id, tin_id);
    assert_eq!(reference.path, relative_path);
    assert_eq!(reference.owner, 100);
    assert_eq!(reference.wrapped_key, wrapped_key);

    // 4. Verify without owner filter
    let any_ref = Database::get_reference(&conn, &tin_id, None)
        .unwrap()
        .expect("reference row must exist without owner filter");
    assert_eq!(any_ref.owner, 100);
}

// spec: alhalte-cellar/session-verifier
#[tokio::test]
async fn test_session_verifier_authorization() {
    let verifier = InMemorySessionVerifier::new();

    // Initially unauthorized
    assert!(!verifier.verify_session(1, 101).await.unwrap());

    // Allow session
    verifier.allow(1, 101);
    assert!(verifier.verify_session(1, 101).await.unwrap());
    assert!(!verifier.verify_session(1, 999).await.unwrap());
    assert!(!verifier.verify_session(2, 101).await.unwrap());

    // Revoke session
    verifier.revoke(1, 101);
    assert!(!verifier.verify_session(1, 101).await.unwrap());
}

// spec: alhalte-cellar/session-verifier
#[tokio::test]
async fn test_maitre_d_verifier_http_mock() {
    // Spin up a mock HTTP server responding to /api/verify-session
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap();
                    let req_str = String::from_utf8_lossy(&buf[..n]);

                    if req_str.contains("owner=42") && req_str.contains("session_id=100") {
                        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
                        stream.write_all(resp.as_bytes()).await.unwrap();
                    } else {
                        let resp =
                            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\n\r\nUnauthorized";
                        stream.write_all(resp.as_bytes()).await.unwrap();
                    }
                });
            }
        }
    });

    let verifier = MaitreDVerifier::new_http(format!("http://{addr}"));

    // Valid session
    assert!(verifier.verify_session(42, 100).await.unwrap());

    // Invalid session
    assert!(!verifier.verify_session(42, 999).await.unwrap());
}

// spec: alhalte-cellar/session-verifier
#[tokio::test]
async fn test_maitre_d_verifier_unix_socket_mock() {
    let dir = tempdir().unwrap();
    let socket_path = dir.path().join("maitre-d.sock");

    let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap();
                    let req_str = String::from_utf8_lossy(&buf[..n]);

                    if req_str.contains("owner=77") && req_str.contains("session_id=88") {
                        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
                        stream.write_all(resp.as_bytes()).await.unwrap();
                    } else {
                        let resp =
                            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\n\r\nUnauthorized";
                        stream.write_all(resp.as_bytes()).await.unwrap();
                    }
                });
            }
        }
    });

    let verifier = MaitreDVerifier::new_unix(&socket_path);

    // Valid session
    assert!(verifier.verify_session(77, 88).await.unwrap());

    // Invalid session
    assert!(!verifier.verify_session(77, 99).await.unwrap());
}

// spec: alhalte-cellar/session-verifier (Issue #153)
#[test]
fn test_verifier_mode_resolution() {
    // No endpoint and no explicit allow-all is a startup error, not a silent deny-all.
    let err = VerifierMode::resolve(None, None, false).unwrap_err();
    assert!(matches!(err, CellarError::Configuration(_)));

    let allow_all = VerifierMode::resolve(None, None, true).unwrap();
    assert_eq!(allow_all, VerifierMode::AllowAll);
    assert!(allow_all.is_allow_all());

    // A UNIX socket takes precedence over an HTTP URL.
    let socket_path = std::path::PathBuf::from("/run/alhalte/maitre-d.sock");
    assert_eq!(
        VerifierMode::resolve(
            Some("http://127.0.0.1:8080".to_string()),
            Some(socket_path.clone()),
            false,
        )
        .unwrap(),
        VerifierMode::UnixSocket(socket_path.clone())
    );

    let http =
        VerifierMode::resolve(Some("http://127.0.0.1:8080".to_string()), None, false).unwrap();
    assert_eq!(
        http,
        VerifierMode::Http("http://127.0.0.1:8080".to_string())
    );
    assert!(!http.is_allow_all());

    // Startup logging names the selected mode.
    assert!(allow_all.to_string().contains("allow-all"));
    assert!(http.to_string().contains("HTTP"));
    assert!(
        VerifierMode::UnixSocket(socket_path)
            .to_string()
            .contains("UNIX socket")
    );
}

// spec: alhalte-cellar/session-verifier + store-210-upload-required (Issue #153)
#[tokio::test]
async fn test_store_with_maitre_d_verifier_mock() {
    // Loopback-only mock of maitre-d's /api/verify-session endpoint.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    let Ok(n) = stream.read(&mut buf).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let response =
                        if request.contains("owner=42") && request.contains("session_id=100") {
                            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK"
                        } else {
                            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\n\r\nUnauthorized"
                        };
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        }
    });

    let dir = tempdir().unwrap();
    let verifier = Arc::new(MaitreDVerifier::new_http(format!("http://{addr}")));
    let service =
        Arc::new(CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap());
    let (tin_bytes, tin_id) = create_test_tin("stored through maitre-d verifier");

    // A session accepted by maitre-d can check and upload.
    let store_req = StoreRequest {
        owner: 42,
        session_id: 100,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: None,
    };
    assert_eq!(
        service.store_check(&store_req).await.unwrap(),
        StoreStatus::UploadRequired
    );
    let result = service
        .store_payload(42, 100, tin_id, &tin_bytes, None)
        .await
        .unwrap();
    assert!(result.written_to_disk);
    assert!(service.storage().has_tin_file(&tin_id));

    // A session rejected by maitre-d is unauthorized for both store phases.
    let rejected_req = StoreRequest {
        owner: 42,
        session_id: 999,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: None,
    };
    let check_err = service.store_check(&rejected_req).await.unwrap_err();
    assert!(matches!(
        check_err,
        CellarError::Unauthorized {
            owner: 42,
            session_id: 999
        }
    ));
    let payload_err = service
        .store_payload(42, 999, [0xAB; 32], b"rejected", None)
        .await
        .unwrap_err();
    assert!(matches!(
        payload_err,
        CellarError::Unauthorized {
            owner: 42,
            session_id: 999
        }
    ));

    // The rejection surfaces as HTTP 401 through the router.
    let app = service.into_router();
    let req = Request::builder()
        .method("POST")
        .uri("/store")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&rejected_req).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// spec: alhalte-cellar/store-210-upload-required
#[tokio::test]
async fn test_store_status_210_upload_required() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(10, 100);

    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    let (tin_bytes, tin_id) = create_test_tin("content for 210");

    let req = StoreRequest {
        owner: 10,
        session_id: 100,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: None,
    };

    // Store check returns 210 (UploadRequired)
    let status = service.store_check(&req).await.unwrap();
    assert_eq!(status, StoreStatus::UploadRequired);
    assert_eq!(status.as_u16(), 210);

    // Client uploads payload
    let res = service
        .store_payload(10, 100, tin_id, &tin_bytes, None)
        .await
        .unwrap();
    assert!(res.written_to_disk);
    assert!(res.reference_added);

    // Verify file exists on disk
    assert!(service.storage().has_tin_file(&tin_id));
}

// spec: alhalte-cellar/store-211-deduplicated
#[tokio::test]
async fn test_store_status_211_deduplicated() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(10, 100);
    verifier.allow(20, 200);

    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    let (tin_bytes, tin_id) = create_test_tin("shared deduplicated content");

    // Owner 10 uploads first
    let res = service
        .store_payload(10, 100, tin_id, &tin_bytes, None)
        .await
        .unwrap();
    assert!(res.written_to_disk);

    // Owner 20 sends store_check for the exact same tin
    let req2 = StoreRequest {
        owner: 20,
        session_id: 200,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: Some(vec![0xAA, 0xBB]),
    };

    let status = service.store_check(&req2).await.unwrap();
    // Must return 211 (Deduplicated)
    assert_eq!(status, StoreStatus::Deduplicated);
    assert_eq!(status.as_u16(), 211);

    // Verify only ONE file exists in storage
    let files = service.storage().list_tin_files().unwrap();
    assert_eq!(files.len(), 1);

    // But BOTH owners can download the tin!
    let (data10, key10) = service.download(&tin_id, Some(10)).await.unwrap();
    assert_eq!(data10, tin_bytes);
    assert_eq!(key10, None);

    let (data20, key20) = service.download(&tin_id, Some(20)).await.unwrap();
    assert_eq!(data20, tin_bytes);
    assert_eq!(key20, Some(vec![0xAA, 0xBB]));
}

// spec: alhalte-cellar/store-212-already-owned
#[tokio::test]
async fn test_store_status_212_already_owned() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(10, 100);

    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    let (tin_bytes, tin_id) = create_test_tin("content for 212");

    // Owner 10 stores the tin
    service
        .store_payload(10, 100, tin_id, &tin_bytes, None)
        .await
        .unwrap();

    // Owner 10 checks store again for the same tin
    let req = StoreRequest {
        owner: 10,
        session_id: 100,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: None,
    };

    let status = service.store_check(&req).await.unwrap();
    // Must return 212 (AlreadyOwned)
    assert_eq!(status, StoreStatus::AlreadyOwned);
    assert_eq!(status.as_u16(), 212);
}

// spec: alhalte-cellar/anti-oracle
#[tokio::test]
async fn test_anti_oracle_mode() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(10, 100);
    verifier.allow(20, 200);

    // Anti-oracle is ENABLED
    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, true).unwrap();
    let (tin_bytes, tin_id) = create_test_tin("secret anti-oracle content");

    // Owner 10 uploads
    let res10 = service
        .store_payload(10, 100, tin_id, &tin_bytes, None)
        .await
        .unwrap();
    assert!(res10.written_to_disk);

    // Owner 20 checks store: Even though file exists, anti_oracle returns 210!
    let req20 = StoreRequest {
        owner: 20,
        session_id: 200,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: Some(vec![9, 9, 9]),
    };

    let status = service.store_check(&req20).await.unwrap();
    assert_eq!(status, StoreStatus::UploadRequired);
    assert_eq!(status.as_u16(), 210);

    // Owner 20 sends payload: It should be discarded into /dev/null (written_to_disk: false),
    // but reference_added should still be true!
    let res20 = service
        .store_payload(20, 200, tin_id, &tin_bytes, Some(vec![9, 9, 9]))
        .await
        .unwrap();
    assert!(!res20.written_to_disk);
    assert!(res20.reference_added);

    // Owner 20 can now download
    let (data, key) = service.download(&tin_id, Some(20)).await.unwrap();
    assert_eq!(data, tin_bytes);
    assert_eq!(key, Some(vec![9, 9, 9]));
}

// spec: alhalte-cellar/download
#[tokio::test]
async fn test_download_tin_and_wrapped_key() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(1, 10);

    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    let (tin_bytes, tin_id) = create_test_tin("downloadable tin");
    let wrapped_key = vec![11, 22, 33, 44];

    service
        .store_payload(1, 10, tin_id, &tin_bytes, Some(wrapped_key.clone()))
        .await
        .unwrap();

    let (downloaded_bytes, downloaded_key) = service.download(&tin_id, Some(1)).await.unwrap();
    assert_eq!(downloaded_bytes, tin_bytes);
    assert_eq!(downloaded_key, Some(wrapped_key));

    // Non-existent tin download returns NotFound
    let missing_id = [0xFFu8; 32];
    assert!(service.download(&missing_id, None).await.is_err());
}

// spec: alhalte-cellar/unregister
#[tokio::test]
async fn test_unregister_removes_ownership_reference() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(1, 10);
    verifier.allow(2, 20);

    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    let (tin_bytes, tin_id) = create_test_tin("content to unregister");

    service
        .store_payload(1, 10, tin_id, &tin_bytes, None)
        .await
        .unwrap();
    service
        .store_payload(2, 20, tin_id, &tin_bytes, None)
        .await
        .unwrap();

    // Owner 1 unregisters
    let unregistered = service.unregister(1, 10, &tin_id).await.unwrap();
    assert!(unregistered);

    // Owner 1 can no longer download with owner=1 filter
    assert!(service.download(&tin_id, Some(1)).await.is_err());

    // Owner 2 still retains reference and can download
    let (data, _) = service.download(&tin_id, Some(2)).await.unwrap();
    assert_eq!(data, tin_bytes);

    // Entity file is STILL in storage (not deleted until GC)
    assert!(service.storage().has_tin_file(&tin_id));
}

// spec: alhalte-cellar/gc
#[tokio::test]
async fn test_garbage_collection() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(1, 10);

    let service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    let (tin_bytes1, tin_id1) = create_test_tin("retained content");
    let (tin_bytes2, tin_id2) = create_test_tin("orphaned content");

    service
        .store_payload(1, 10, tin_id1, &tin_bytes1, None)
        .await
        .unwrap();
    service
        .store_payload(1, 10, tin_id2, &tin_bytes2, None)
        .await
        .unwrap();

    // Unregister tin_id2 so it becomes orphaned
    service.unregister(1, 10, &tin_id2).await.unwrap();

    // Pre-GC: both files exist on disk
    assert!(service.storage().has_tin_file(&tin_id1));
    assert!(service.storage().has_tin_file(&tin_id2));

    // Run GC
    let stats = service.run_gc().await.unwrap();
    assert_eq!(stats.scanned_files, 2);
    assert_eq!(stats.deleted_files, 1);
    assert!(stats.freed_bytes > 0);

    // Post-GC: tin_id1 remains, tin_id2 is gone!
    assert!(service.storage().has_tin_file(&tin_id1));
    assert!(!service.storage().has_tin_file(&tin_id2));
}

// spec: alhalte-cellar/http-api
#[tokio::test]
async fn test_http_api_flow() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(100, 1000);
    verifier.allow(200, 2000);

    let service =
        Arc::new(CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap());
    let app = service.clone().into_router();

    let (tin_bytes, tin_id) = create_test_tin("HTTP API test payload");
    let hex_id = format_tin_id(&tin_id);

    // 1. POST /store -> Should return 210
    let store_req = StoreRequest {
        owner: 100,
        session_id: 1000,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: Some(vec![1, 2, 3]),
    };

    let req = Request::builder()
        .method("POST")
        .uri("/store")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&store_req).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 210);

    // 2. PUT /store/{tin_id} -> Upload body
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/store/{hex_id}"))
        .header("X-Alhalte-Owner", "100")
        .header("X-Alhalte-Session-Id", "1000")
        .header("X-Alhalte-Wrapped-Key", "010203")
        .header("Content-Type", "application/octet-stream")
        .body(Body::from(tin_bytes.clone()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 3. GET /download/{tin_id} -> Download body and check header
    let req = Request::builder()
        .method("GET")
        .uri(format!("/download/{hex_id}?owner=100"))
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("X-Alhalte-Wrapped-Key").unwrap(),
        "010203"
    );
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes.as_ref(), &tin_bytes);

    // 4. POST /store by second owner 200 -> Deduplication (status 211)
    let store_req2 = StoreRequest {
        owner: 200,
        session_id: 2000,
        tin_id,
        size: tin_bytes.len() as u64,
        wrapped_key: None,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/store")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&store_req2).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 211);

    // 5. POST /store by owner 100 again -> Already owned (status 212)
    let req = Request::builder()
        .method("POST")
        .uri("/store")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&store_req).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 212);

    // 6. POST /unregister by owner 100
    let unreg = UnregisterRequest {
        owner: 100,
        session_id: 1000,
        tin_id,
    };
    let req = Request::builder()
        .method("POST")
        .uri("/unregister")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&unreg).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 7. POST /gc -> Garbage collection trigger
    let req = Request::builder()
        .method("POST")
        .uri("/gc")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// spec: alhalte-cellar/handlers raises the upload body limit above Axum's 2 MiB default (Issue #109 / #146)
#[tokio::test]
async fn test_store_accepts_body_larger_than_default_limit() {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    verifier.allow(100, 1000);

    let service =
        Arc::new(CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap());
    let app = service.clone().into_router();

    // Deliberately uncompressed so the wire body really exceeds 2 MiB.
    let text = "x".repeat(3 * 1024 * 1024 + 1);
    let (tin_bytes, tin_id) = Tin::from_string(&text)
        .encode_plaintext(tin::Compression::None)
        .unwrap();
    assert!(
        tin_bytes.len() > 2 * 1024 * 1024,
        "test payload must exceed the default body limit"
    );
    let hex_id = format_tin_id(&tin_id);

    let req = Request::builder()
        .method("PUT")
        .uri(format!("/store/{hex_id}"))
        .header("X-Alhalte-Owner", "100")
        .header("X-Alhalte-Session-Id", "1000")
        .header("Content-Type", "application/octet-stream")
        .body(Body::from(tin_bytes.clone()))
        .unwrap();

    // Axum's default 2 MiB limit would answer 413 here.
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/download/{hex_id}?owner=100"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), tin_bytes.as_slice());
}

// spec: alhalte-cellar/cli
#[test]
fn test_cli_bistrot_argument_compatibility() {
    // Matching arguments generated by bistrot:
    // ["--host", "127.0.0.1", "--port", "8081", "--storage-path", "/custom/tin", "--db-path", "/custom/cellar.sqlite3"]
    let args1 = [
        "alhalte-cellar",
        "--host",
        "0.0.0.0",
        "--port",
        "9090",
        "--storage-path",
        "/custom/tin",
        "--db-path",
        "/custom/cellar.sqlite3",
    ];

    let parsed1 = alhalte_cellar::parse_cli_args(args1).unwrap();
    assert_eq!(parsed1.host, "0.0.0.0");
    assert_eq!(parsed1.port, 9090);
    assert_eq!(
        parsed1.storage_path,
        std::path::PathBuf::from("/custom/tin")
    );
    assert_eq!(
        parsed1.db_path,
        std::path::PathBuf::from("/custom/cellar.sqlite3")
    );
    assert!(!parsed1.anti_oracle);

    // With --anti-oracle flag
    let args2 = ["alhalte-cellar", "--port", "8081", "--anti-oracle"];
    let parsed2 = alhalte_cellar::parse_cli_args(args2).unwrap();
    assert!(parsed2.anti_oracle);

    // With --anti-oracle true
    let args3 = ["alhalte-cellar", "--port", "8081", "--anti-oracle", "true"];
    let parsed3 = alhalte_cellar::parse_cli_args(args3).unwrap();
    assert!(parsed3.anti_oracle);

    // With --anti-oracle false
    let args4 = ["alhalte-cellar", "--port", "8081", "--anti-oracle", "false"];
    let parsed4 = alhalte_cellar::parse_cli_args(args4).unwrap();
    assert!(!parsed4.anti_oracle);

    // Session verifier flags generated by bistrot (Issue #153)
    let args5 = [
        "alhalte-cellar",
        "--port",
        "8081",
        "--maitre-d-url",
        "http://127.0.0.1:8080",
        "--allow-all-sessions",
    ];
    let parsed5 = alhalte_cellar::parse_cli_args(args5).unwrap();
    assert_eq!(
        parsed5.maitre_d_url.as_deref(),
        Some("http://127.0.0.1:8080")
    );
    assert!(parsed5.allow_all_sessions);
    assert_eq!(parsed5.maitre_d_socket, None);

    let args6 = [
        "alhalte-cellar",
        "--port",
        "8081",
        "--maitre-d-socket",
        "/run/alhalte/maitre-d.sock",
    ];
    let parsed6 = alhalte_cellar::parse_cli_args(args6).unwrap();
    assert_eq!(
        parsed6.maitre_d_socket,
        Some(std::path::PathBuf::from("/run/alhalte/maitre-d.sock"))
    );
    assert!(!parsed6.allow_all_sessions);
}
