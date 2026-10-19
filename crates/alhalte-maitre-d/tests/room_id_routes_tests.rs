//! Room-scoped route contract tests for Issue #154.
//!
//! These tests exercise the real Axum routes (not just the parser) and prove
//! that canonical 16-hex room IDs work while arbitrary IDs are rejected with
//! HTTP 400.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tempfile::tempdir;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use alhalte_banquette::{ClientFrame, ServerFrame};
use alhalte_maitre_d::{
    AppState, Database, InProcessBanquetteSpawner, MockOAuthProvider, RoomBridgeManager,
    create_router,
};

async fn spawn_router() -> (std::net::SocketAddr, tempfile::TempDir, Database) {
    let temp_dir = tempdir().unwrap();
    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        temp_dir.path().to_path_buf(),
        8,
    ));
    let db = Database::new_in_memory().unwrap();
    let state = AppState::new(
        db.clone(),
        "salt",
        Arc::new(MockOAuthProvider::new()),
        RoomBridgeManager::new(spawner, db.clone()),
    );
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, temp_dir, db)
}

// spec: alhalte-maitre-d/room-id invalid IDs return 400 on room-scoped routes
#[tokio::test]
async fn invalid_room_ids_return_400_on_room_scoped_routes() {
    let (addr, _temp_dir, _db) = spawn_router().await;
    let client = reqwest::Client::new();

    for path in [
        "/api/room/room-1/permissions",
        "/room/room-1/permissions",
        "/api/room/room-1/export/zip",
        "/room/room-1/export/zip",
    ] {
        let resp = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{path} must reject a non-room-ID path segment"
        );
    }

    for path in ["/ws/room/room-1", "/room/room-1", "/room/room-1/ws"] {
        let result = connect_async(format!("ws://{addr}{path}")).await;
        match result {
            Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
                assert_eq!(
                    resp.status(),
                    axum::http::StatusCode::BAD_REQUEST,
                    "{path} must reject a non-room-ID path segment"
                );
            }
            other => panic!("expected HTTP 400 for {path}, got {other:?}"),
        }
    }

    let invalid_query = connect_async(format!("ws://{addr}/ws?room=room-1")).await;
    match invalid_query {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
            assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        }
        other => panic!("expected HTTP 400 for /ws?room=room-1, got {other:?}"),
    }
}

// spec: alhalte-maitre-d/room-id /ws?room=<hex> accepts canonical IDs
#[tokio::test]
async fn websocket_query_accepts_canonical_hex_room_id() {
    let (addr, _temp_dir, db) = spawn_router().await;
    let room_id = "fedcba9876543210";
    let account = db
        .get_or_create_account("ws-query-sub", "QueryUser", None, 1000)
        .unwrap();
    let session = db.create_session(account.account_id, 1000).unwrap();
    db.create_room(room_id, "Query room", account.account_id, 1000)
        .unwrap();

    let (mut ws_stream, _) = connect_async(format!(
        "ws://{addr}/ws?room={room_id}&account_id={}&session_id={}",
        account.account_id, session.session_id
    ))
    .await
    .unwrap();
    let list_req = serde_json::to_string(&ClientFrame::ListScenes).unwrap();
    ws_stream
        .send(WsMessage::Text(list_req.into()))
        .await
        .unwrap();

    let mut received = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::SceneList { scenes }) =
                serde_json::from_str::<ServerFrame>(&text)
        {
            assert!(scenes.contains(&"/".to_string()));
            received = true;
            break;
        }
    }
    assert!(received, "expected a SceneList over /ws?room=<hex>");
}

// spec: alhalte-maitre-d/room-id permission routes use the canonical hex form
#[tokio::test]
async fn permission_routes_accept_canonical_hex_room_id() {
    let (addr, _temp_dir, _db) = spawn_router().await;
    let room_id = "deadbeefcafebabe";

    let resp = reqwest::get(format!("http://{addr}/api/room/{room_id}/permissions"))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let permissions: Vec<alhalte_maitre_d::RoomPermissionItem> = resp.json().await.unwrap();
    assert!(permissions.is_empty());
}
