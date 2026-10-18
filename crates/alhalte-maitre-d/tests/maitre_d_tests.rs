//! Comprehensive test suite for `alhalte-maitre-d`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, TimeZone};
use futures_util::{SinkExt, StreamExt};
use tempfile::tempdir;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use alhalte_banquette::{ClientFrame, ServerFrame};
use alhalte_maitre_d::{
    AccountSearchItem, AppState, DEBUG_SUB_CIPHERTEXT, Database, IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID,
    IDENT_ALHALTE_MAITRE_D_SESSION_ID, InProcessBanquetteSpawner,
    LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, LABEL_ALHALTE_MAITRE_D_SESSION_ID, MockOAuthProvider,
    RoomBridgeManager, RoomPermissionItem, RoomRole, SetRoomPermissionRequest, account_id_from_tin,
    account_id_to_tin, cleanup_sessions_at, cleanup_sessions_with_cutoff, create_router,
    decrypt_sub, encrypt_sub, extract_session_id_from_cookie, parse_cli_args_from,
    parse_session_id_from_cookie_str, previous_4am_local, session_id_from_tin, session_id_to_tin,
};

// spec: alhalte-maitre-d/labels
#[test]
fn test_labels_and_tin_conversions() {
    assert_eq!(
        IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID,
        "alhalte/maitre-d/account-id"
    );
    assert_eq!(
        IDENT_ALHALTE_MAITRE_D_SESSION_ID,
        "alhalte/maitre-d/session-id"
    );

    // Verify label hashes match tin::label_hash
    assert_eq!(
        LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID,
        tin::label_hash(IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID)
    );
    assert_eq!(
        LABEL_ALHALTE_MAITRE_D_SESSION_ID,
        tin::label_hash(IDENT_ALHALTE_MAITRE_D_SESSION_ID)
    );

    // Account ID roundtrip
    let account_id: u64 = 0x0123_4567_89AB_CDEF;
    let account_tin = account_id_to_tin(account_id);
    assert_eq!(account_tin.label, LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID);
    assert_eq!(account_tin.payload.len(), 8);
    let decoded_account_id = account_id_from_tin(&account_tin).unwrap();
    assert_eq!(decoded_account_id, account_id);

    // Session ID roundtrip
    let session_id: u64 = 0xFEDC_BA98_7654_3210;
    let session_tin = session_id_to_tin(session_id);
    assert_eq!(session_tin.label, LABEL_ALHALTE_MAITRE_D_SESSION_ID);
    assert_eq!(session_tin.payload.len(), 8);
    let decoded_session_id = session_id_from_tin(&session_tin).unwrap();
    assert_eq!(decoded_session_id, session_id);

    // Invalid label error
    let invalid_tin = tin::Tin::from_u64(42);
    assert!(account_id_from_tin(&invalid_tin).is_err());
    assert!(session_id_from_tin(&invalid_tin).is_err());

    // Invalid length error
    let short_tin = tin::Tin::new(LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, vec![1, 2, 3]);
    assert!(account_id_from_tin(&short_tin).is_err());
}

// spec: alhalte-maitre-d/crypto
#[test]
fn test_sub_deterministic_encryption() {
    let salt = "unique-instance-salt-12345";
    let sub = "109876543210987654321";

    // Deterministic: multiple encryptions with identical salt yield identical ciphertext
    let enc1 = encrypt_sub(salt, sub).unwrap();
    let enc2 = encrypt_sub(salt, sub).unwrap();
    assert_eq!(enc1, enc2);

    // Different salt yields different ciphertext
    let enc_diff_salt = encrypt_sub("another-salt", sub).unwrap();
    assert_ne!(enc1, enc_diff_salt);

    // Reversible: decrypts back to original sub
    let dec = decrypt_sub(salt, &enc1).unwrap();
    assert_eq!(dec, sub);

    // Wrong salt fails decryption
    assert!(decrypt_sub("wrong-salt", &enc1).is_err());

    // Debug sub ciphertext "0" decrypts directly as "0"
    assert_eq!(decrypt_sub(salt, DEBUG_SUB_CIPHERTEXT).unwrap(), "0");
}

// spec: alhalte-maitre-d/accounts
#[test]
fn test_sqlite_accounts_management() {
    let db = Database::new_in_memory().unwrap();

    let salt = "instance-salt";
    let sub = "google-sub-alice";
    let enc_sub = encrypt_sub(salt, sub).unwrap();
    let icon_id = [0xAAu8; 32];

    // Create new account
    let acc1 = db
        .get_or_create_account(&enc_sub, "Alice", Some(icon_id), 1000)
        .unwrap();
    assert_eq!(acc1.account_id, 1);
    assert_eq!(acc1.handle_name, "Alice");
    assert_eq!(acc1.icon_tin_id, Some(icon_id));

    // Calling again returns existing account
    let acc1_repeat = db
        .get_or_create_account(&enc_sub, "Alice", None, 1005)
        .unwrap();
    assert_eq!(acc1_repeat.account_id, 1);

    // Query by ID
    let fetched = db.get_account(1).unwrap().unwrap();
    assert_eq!(fetched.handle_name, "Alice");
    assert_eq!(fetched.icon_tin_id, Some(icon_id));

    // Update profile: update name and remove icon
    db.update_account_profile(1, Some("Alice New"), Some(None))
        .unwrap();
    let updated = db.get_account(1).unwrap().unwrap();
    assert_eq!(updated.handle_name, "Alice New");
    assert_eq!(updated.icon_tin_id, None);

    // Update non-existent account returns error
    assert!(db.update_account_profile(999, Some("Ghost"), None).is_err());
}

// spec: alhalte-maitre-d/sessions
#[test]
fn test_sqlite_sessions_and_heartbeats() {
    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-enc", "Bob", None, 1000)
        .unwrap();

    // Create session
    let session = db.create_session(acc.account_id, 1000).unwrap();
    assert_ne!(session.session_id, 0);
    assert_eq!(session.account_id, acc.account_id);
    assert_eq!(session.last_heartbeat, 1000);

    // Check validity
    assert!(
        db.is_session_valid(acc.account_id, session.session_id)
            .unwrap()
    );
    // Invalid with different account
    assert!(!db.is_session_valid(999, session.session_id).unwrap());
    // Invalid with non-existent session
    assert!(!db.is_session_valid(acc.account_id, 9999).unwrap());

    // Record heartbeat
    assert!(db.record_heartbeat(session.session_id, 1100).unwrap());
    let fetched_session = db.get_session(session.session_id).unwrap().unwrap();
    assert_eq!(fetched_session.last_heartbeat, 1100);

    // Delete session
    assert!(db.delete_session(session.session_id).unwrap());
    assert!(
        !db.is_session_valid(acc.account_id, session.session_id)
            .unwrap()
    );
}

// spec: alhalte-maitre-d/previous-4am-local
#[test]
fn test_previous_4am_local_calculation() {
    // Reference date: 2026-10-17
    // Case 1: 15:00:00 (after 4 AM today) -> previous 4am is today 04:00:00
    let dt_15 = Local
        .with_ymd_and_hms(2026, 10, 17, 15, 0, 0)
        .single()
        .unwrap();
    let prev_15 = previous_4am_local(dt_15);
    assert_eq!(
        prev_15,
        Local
            .with_ymd_and_hms(2026, 10, 17, 4, 0, 0)
            .single()
            .unwrap()
    );

    // Case 2: 02:30:00 (before 4 AM today) -> previous 4am is yesterday 04:00:00 (2026-10-16 04:00)
    let dt_02 = Local
        .with_ymd_and_hms(2026, 10, 17, 2, 30, 0)
        .single()
        .unwrap();
    let prev_02 = previous_4am_local(dt_02);
    assert_eq!(
        prev_02,
        Local
            .with_ymd_and_hms(2026, 10, 16, 4, 0, 0)
            .single()
            .unwrap()
    );

    // Case 3: 04:00:00 (exact time of 4am run) -> previous 4am is yesterday 04:00:00 (2026-10-16 04:00)
    let dt_04_exact = Local
        .with_ymd_and_hms(2026, 10, 17, 4, 0, 0)
        .single()
        .unwrap();
    let prev_04_exact = previous_4am_local(dt_04_exact);
    assert_eq!(
        prev_04_exact,
        Local
            .with_ymd_and_hms(2026, 10, 16, 4, 0, 0)
            .single()
            .unwrap()
    );

    // Case 4: 04:02:00 (within 5-minute execution window) -> previous 4am is yesterday 04:00:00
    let dt_04_02 = Local
        .with_ymd_and_hms(2026, 10, 17, 4, 2, 0)
        .single()
        .unwrap();
    let prev_04_02 = previous_4am_local(dt_04_02);
    assert_eq!(
        prev_04_02,
        Local
            .with_ymd_and_hms(2026, 10, 16, 4, 0, 0)
            .single()
            .unwrap()
    );
}

// spec: alhalte-maitre-d/session-4am-cleanup
#[test]
fn test_deterministic_session_cleanup_at_4am() {
    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-clean", "Charlie", None, 1000)
        .unwrap();

    let day1_04am = Local
        .with_ymd_and_hms(2026, 10, 16, 4, 0, 0)
        .single()
        .unwrap();
    let day2_04am = Local
        .with_ymd_and_hms(2026, 10, 17, 4, 0, 0)
        .single()
        .unwrap();

    // Session 1: Last heartbeat was at 2026-10-16 02:00 (BEFORE Day 1 4am)
    let s1 = db.create_session(acc.account_id, 100).unwrap();
    let s1_time = Local
        .with_ymd_and_hms(2026, 10, 16, 2, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    db.record_heartbeat(s1.session_id, s1_time).unwrap();

    // Session 2: Last heartbeat was at 2026-10-16 10:00 (AFTER Day 1 4am, active yesterday)
    let s2 = db.create_session(acc.account_id, 200).unwrap();
    let s2_time = Local
        .with_ymd_and_hms(2026, 10, 16, 10, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    db.record_heartbeat(s2.session_id, s2_time).unwrap();

    // Verify cleanup_sessions_with_cutoff directly
    let deleted_direct = cleanup_sessions_with_cutoff(&db, day1_04am).unwrap();
    assert_eq!(deleted_direct, 1);

    assert!(!db.is_session_valid(acc.account_id, s1.session_id).unwrap());
    assert!(db.is_session_valid(acc.account_id, s2.session_id).unwrap());

    // Calling cleanup_sessions_at at Day 2 04:00 AM (cutoff = Day 1 04:00 AM) deletes 0 additional sessions
    let deleted_count = cleanup_sessions_at(&db, day2_04am).unwrap();
    assert_eq!(deleted_count, 0);

    // Advance to Day 3 04:00 AM (2026-10-18 04:00)
    // The previous 4am is Day 2 04:00 AM (2026-10-17 04:00)
    // s2 (last active 10-16 10:00 < 10-17 04:00) now has had no heartbeat since Day 2 04:00 -> DELETED!
    let day3_04am = Local
        .with_ymd_and_hms(2026, 10, 18, 4, 0, 0)
        .single()
        .unwrap();
    let deleted_count2 = cleanup_sessions_at(&db, day3_04am).unwrap();
    assert_eq!(deleted_count2, 1);

    assert!(!db.is_session_valid(acc.account_id, s2.session_id).unwrap());
}

// spec: alhalte-maitre-d/debug-account
#[tokio::test]
async fn test_debug_account_creation() {
    let db = Database::new_in_memory().unwrap();
    let now = chrono::Utc::now().timestamp();

    // Direct database method
    let debug_acc = db.create_debug_account("DebugUser", None, now).unwrap();
    assert_eq!(debug_acc.encrypted_sub, DEBUG_SUB_CIPHERTEXT);
    assert_eq!(debug_acc.encrypted_sub, "0");
    assert_eq!(debug_acc.handle_name, "DebugUser");

    let session = db.create_session(debug_acc.account_id, now).unwrap();
    assert!(
        db.is_session_valid(debug_acc.account_id, session.session_id)
            .unwrap()
    );

    // HTTP endpoint test
    let spawner =
        Arc::new(alhalte_maitre_d::InProcessBanquetteSpawner::new_with_in_memory_cellar("/tmp", 8));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/debug/account"))
        .json(&serde_json::json!({
            "handle_name": "HttpDebugUser"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let auth_res: alhalte_maitre_d::AuthResponse = resp.json().await.unwrap();
    assert_eq!(auth_res.account.encrypted_sub, "0");
    assert_eq!(auth_res.account.handle_name, "HttpDebugUser");

    // Session is valid
    assert!(
        db.is_session_valid(auth_res.account.account_id, auth_res.session_id)
            .unwrap()
    );
}

// spec: alhalte-maitre-d/session-verification-endpoint
#[tokio::test]
async fn test_session_verification_http_endpoint() {
    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-verify", "Dave", None, 1000)
        .unwrap();
    let session = db.create_session(acc.account_id, 1000).unwrap();

    let spawner =
        Arc::new(alhalte_maitre_d::InProcessBanquetteSpawner::new_with_in_memory_cellar("/tmp", 8));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    // Valid query: owner=...&session_id=...
    let resp = client
        .get(format!(
            "http://{addr}/api/verify-session?owner={}&session_id={}",
            acc.account_id, session.session_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Invalid query: wrong session_id
    let resp_invalid = client
        .get(format!(
            "http://{addr}/api/verify-session?owner={}&session_id=99999",
            acc.account_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_invalid.status(), reqwest::StatusCode::UNAUTHORIZED);
}

// spec: alhalte-maitre-d/maitre-d-verifier-cellar-interop
#[tokio::test]
async fn test_interoperability_with_cellar_maitre_d_verifier() {
    use alhalte_cellar::{MaitreDVerifier, SessionVerifier};

    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-cellar", "Eve", None, 1000)
        .unwrap();
    let session = db.create_session(acc.account_id, 1000).unwrap();

    let spawner =
        Arc::new(alhalte_maitre_d::InProcessBanquetteSpawner::new_with_in_memory_cellar("/tmp", 8));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Create alhalte-cellar's MaitreDVerifier communicating over HTTP
    let verifier = MaitreDVerifier::new_http(format!("http://{addr}"));

    // Verify valid session
    let is_valid = verifier
        .verify_session(acc.account_id, session.session_id)
        .await
        .unwrap();
    assert!(is_valid);

    // Verify invalid session
    let is_invalid = verifier
        .verify_session(acc.account_id, 88888)
        .await
        .unwrap();
    assert!(!is_invalid);
}

// spec: alhalte-maitre-d/oauth-exchange
#[tokio::test]
async fn test_oauth_flow_and_account_linkage() {
    let db = Database::new_in_memory().unwrap();
    let mock_oauth = Arc::new(MockOAuthProvider::new());
    mock_oauth.register_code("auth-code-123", "google-sub-frank");
    mock_oauth.register_token("id-token-abc", "google-sub-frank");

    let spawner =
        Arc::new(alhalte_maitre_d::InProcessBanquetteSpawner::new_with_in_memory_cellar("/tmp", 8));
    let room_manager = RoomBridgeManager::new(spawner);
    let salt = "my-test-salt";
    let state = AppState::new(db.clone(), salt, mock_oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Authorization URL
    let url_resp = client
        .get(format!(
            "http://{addr}/auth/google/url?redirect_uri=http://app.com/cb&state=xyz"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(url_resp.status(), reqwest::StatusCode::OK);

    // 2. Code exchange via POST /auth/google/callback
    let cb_resp = client
        .post(format!("http://{addr}/auth/google/callback"))
        .json(&serde_json::json!({
            "code": "auth-code-123",
            "redirect_uri": "http://app.com/cb",
            "handle_name": "Frank"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cb_resp.status(), reqwest::StatusCode::OK);
    let auth_data: alhalte_maitre_d::AuthResponse = cb_resp.json().await.unwrap();
    assert_eq!(auth_data.account.handle_name, "Frank");

    // The sub stored in DB is encrypted with instance salt
    let expected_enc_sub = encrypt_sub(salt, "google-sub-frank").unwrap();
    assert_eq!(auth_data.account.encrypted_sub, expected_enc_sub);
    assert_eq!(
        decrypt_sub(salt, &auth_data.account.encrypted_sub).unwrap(),
        "google-sub-frank"
    );

    // 3. Repeated login with ID token returns the same account ID with a fresh session ID
    let token_resp = client
        .post(format!("http://{addr}/auth/google/token"))
        .json(&serde_json::json!({
            "id_token": "id-token-abc"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(token_resp.status(), reqwest::StatusCode::OK);
    let auth_data2: alhalte_maitre_d::AuthResponse = token_resp.json().await.unwrap();

    assert_eq!(auth_data2.account.account_id, auth_data.account.account_id);
    assert_ne!(auth_data2.session_id, auth_data.session_id);
    assert!(
        db.is_session_valid(auth_data2.account.account_id, auth_data2.session_id)
            .unwrap()
    );
}

// spec: alhalte-maitre-d/cli-args
#[test]
fn test_cli_bistrot_argument_compatibility() {
    let args = [
        "alhalte-maitre-d",
        "--host",
        "127.0.0.1",
        "--port",
        "9000",
        "--db-path",
        "/var/lib/alhalte/maitre-d.sqlite3",
        "--client-id",
        "google-client-12345",
        "--client-secret",
        "super-secret-oauth",
        "--salt",
        "instance-salt-67890",
        "--socket-dir",
        "/run/alhalte",
    ];

    let parsed = parse_cli_args_from(args).unwrap();
    assert_eq!(parsed.host, "127.0.0.1");
    assert_eq!(parsed.port, 9000);
    assert_eq!(
        parsed.db_path,
        std::path::PathBuf::from("/var/lib/alhalte/maitre-d.sqlite3")
    );
    assert_eq!(parsed.client_id, "google-client-12345");
    assert_eq!(parsed.client_secret, "super-secret-oauth");
    assert_eq!(parsed.salt, "instance-salt-67890");
    assert_eq!(parsed.socket_dir, std::path::PathBuf::from("/run/alhalte"));
}

// spec: alhalte-maitre-d/websocket-demux-and-spawner
#[tokio::test]
async fn test_websocket_demux_and_banquette_spawner() {
    let temp_dir = tempdir().unwrap();
    let socket_dir = temp_dir.path().to_path_buf();

    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-ws", "Grace", None, 1000)
        .unwrap();
    let session = db.create_session(acc.account_id, 1000).unwrap();

    // Use InProcessBanquetteSpawner
    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 777;

    // Connect via WebSocket to /ws/room/{room_id} with credentials
    let ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        acc.account_id, session.session_id
    );

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();

    // Immediately send ClientFrame::ListScenes
    // Banquette is being launched concurrently; this message is queued and flushed upon socket connection!
    let list_req = serde_json::to_string(&ClientFrame::ListScenes).unwrap();
    ws_stream
        .send(WsMessage::Text(list_req.into()))
        .await
        .unwrap();

    // Await response from banquette
    let mut received_scene_list = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::SceneList { scenes }) =
                serde_json::from_str::<ServerFrame>(&text)
        {
            // Root virtual scene "/" must always be present
            assert!(scenes.contains(&"/".to_string()));
            received_scene_list = true;
            break;
        }
    }

    assert!(
        received_scene_list,
        "Failed to receive SceneList response from spawned banquette"
    );

    // Send ClientFrame::CreateScene
    let create_scene = serde_json::to_string(&ClientFrame::CreateScene {
        scene_id: "tavern".to_string(),
        inherit_juices: true,
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(create_scene.into()))
        .await
        .unwrap();

    // Query scenes again
    let list_req2 = serde_json::to_string(&ClientFrame::ListScenes).unwrap();
    ws_stream
        .send(WsMessage::Text(list_req2.into()))
        .await
        .unwrap();

    let mut created_scene_found = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::SceneList { scenes }) =
                serde_json::from_str::<ServerFrame>(&text)
            && scenes.contains(&"tavern".to_string())
        {
            created_scene_found = true;
            break;
        }
    }

    assert!(
        created_scene_found,
        "Created scene 'tavern' not listed in banquette scene list"
    );

    // Close WebSocket
    ws_stream.close(None).await.unwrap();
}

// spec: alhalte-maitre-d/export zip
#[tokio::test]
async fn test_room_zip_export_endpoints() {
    let temp_dir = tempdir().unwrap();
    let socket_dir = temp_dir.path().to_path_buf();

    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-export", "Henry", None, 1000)
        .unwrap();
    let session = db.create_session(acc.account_id, 1000).unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 888;
    let client = reqwest::Client::new();

    // Populate data via WebSocket connection
    let ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        acc.account_id, session.session_id
    );
    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();

    // 1. Create character
    let mut char_map = std::collections::HashMap::new();
    alhalte_banquette::character::set_character_name(&mut char_map, "Hero");
    alhalte_banquette::character::set_character_owner(&mut char_map, acc.account_id);
    let create_char_frame =
        serde_json::to_string(&ClientFrame::CreateCharacter { data: char_map }).unwrap();
    ws_stream
        .send(WsMessage::Text(create_char_frame.into()))
        .await
        .unwrap();

    // 2. Create scene "dungeon"
    let create_scene_frame = serde_json::to_string(&ClientFrame::CreateScene {
        scene_id: "dungeon".to_string(),
        inherit_juices: true,
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(create_scene_frame.into()))
        .await
        .unwrap();

    // 3. Update scene state in "dungeon"
    let state_tin = tin::Tin::from_string("Dark and damp");
    let update_state_frame = serde_json::to_string(&ClientFrame::UpdateState {
        scene_id: "dungeon".to_string(),
        key: "atmosphere::desc".to_string(),
        value: state_tin,
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(update_state_frame.into()))
        .await
        .unwrap();

    // 4. Append a log entry in "dungeon"
    let log_tin = tin::Tin::from_string("A torch flickers on the stone wall.");
    let append_log_frame = serde_json::to_string(&ClientFrame::AppendLog {
        scene_id: "dungeon".to_string(),
        entry: Some(log_tin),
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(append_log_frame.into()))
        .await
        .unwrap();

    // Wait a brief moment for frames to be processed by banquette
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Test 1: Full export via GET /api/room/{room_id}/export/zip
    let full_url = format!(
        "http://{addr}/api/room/{room_id}/export/zip?type=full&account_id={}&session_id={}",
        acc.account_id, session.session_id
    );
    let resp = client.get(&full_url).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert_eq!(
        resp.headers().get("content-disposition").unwrap(),
        &format!("attachment; filename=\"room_{room_id}_full.zip\"")
    );

    let body = resp.bytes().await.unwrap();
    // Validate ZIP magic header: PK\x03\x04
    assert!(body.len() >= 4);
    assert_eq!(&body[0..4], b"PK\x03\x04");

    // Load and verify with RoomArchive::load_from_slice
    let archive = alhalte_kitchen::RoomArchive::load_from_slice(&body, None)
        .expect("Failed to load archive from slice");
    assert_eq!(archive.room_id, Some(room_id));
    assert!(archive.scenes.contains_key("dungeon"));
    let dungeon_scene = archive.scenes.get("dungeon").unwrap();
    // In full export, scene states must NOT be empty
    assert!(!dungeon_scene.states.is_empty());
    assert!(dungeon_scene.states.contains_key("atmosphere::desc"));
    assert!(!archive.characters.is_empty());

    // Test 2: Logs export via GET /room/{room_id}/export/zip (alternative path)
    let logs_url = format!(
        "http://{addr}/room/{room_id}/export/zip?type=logs&account_id={}&session_id={}",
        acc.account_id, session.session_id
    );
    let logs_resp = client.get(&logs_url).send().await.unwrap();
    assert_eq!(logs_resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        logs_resp.headers().get("content-type").unwrap(),
        "application/zip"
    );
    assert_eq!(
        logs_resp.headers().get("content-disposition").unwrap(),
        &format!("attachment; filename=\"room_{room_id}_logs.zip\"")
    );

    let logs_body = logs_resp.bytes().await.unwrap();
    assert_eq!(&logs_body[0..4], b"PK\x03\x04");

    let logs_archive = alhalte_kitchen::RoomArchive::load_from_slice(&logs_body, None)
        .expect("Failed to load logs archive from slice");
    assert_eq!(logs_archive.room_id, Some(room_id));
    assert!(logs_archive.scenes.contains_key("dungeon"));
    let logs_dungeon = logs_archive.scenes.get("dungeon").unwrap();
    // In logs export, scene states MUST be empty!
    assert!(logs_dungeon.states.is_empty());
    // Characters repository must be retained
    assert!(!logs_archive.characters.is_empty());

    // Test 3: Default type is "full" when type query param is omitted
    let default_url = format!(
        "http://{addr}/api/room/{room_id}/export/zip?account_id={}&session_id={}",
        acc.account_id, session.session_id
    );
    let default_resp = client.get(&default_url).send().await.unwrap();
    assert_eq!(default_resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        default_resp.headers().get("content-disposition").unwrap(),
        &format!("attachment; filename=\"room_{room_id}_full.zip\"")
    );

    // Test 4: Invalid type query parameter returns 400 Bad Request
    let invalid_type_url = format!(
        "http://{addr}/api/room/{room_id}/export/zip?type=invalid&account_id={}&session_id={}",
        acc.account_id, session.session_id
    );
    let bad_type_resp = client.get(&invalid_type_url).send().await.unwrap();
    assert_eq!(bad_type_resp.status(), reqwest::StatusCode::BAD_REQUEST);

    // Test 5: Invalid session returns 401 Unauthorized
    let bad_session_url = format!(
        "http://{addr}/api/room/{room_id}/export/zip?type=full&account_id={}&session_id=999999",
        acc.account_id
    );
    let unauth_resp = client.get(&bad_session_url).send().await.unwrap();
    assert_eq!(unauth_resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Test 6: CORS GET request with Origin header receives access-control-allow-origin and ZIP binary
    let origin_resp = client
        .get(&full_url)
        .header("Origin", "http://localhost:5173")
        .send()
        .await
        .unwrap();
    assert_eq!(origin_resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        origin_resp
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "*"
    );
    let origin_body = origin_resp.bytes().await.unwrap();
    assert_eq!(&origin_body[0..4], b"PK\x03\x04");

    // Test 7: CORS OPTIONS preflight request receives access-control-allow-origin
    let preflight_resp = client
        .request(reqwest::Method::OPTIONS, &full_url)
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight_resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        preflight_resp
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "*"
    );

    // Clean up
    ws_stream.close(None).await.unwrap();
}

// spec: alhalte-maitre-d/cookie-session
#[test]
fn test_cookie_session_extraction() {
    assert_eq!(
        parse_session_id_from_cookie_str("session_id=12345"),
        Some(12345)
    );
    assert_eq!(
        parse_session_id_from_cookie_str("theme=dark; session_id=98765; lang=ja"),
        Some(98765)
    );
    assert_eq!(
        parse_session_id_from_cookie_str("session_id=\"55555\""),
        Some(55555)
    );
    assert_eq!(
        parse_session_id_from_cookie_str("session_id=notanumber"),
        None
    );
    assert_eq!(parse_session_id_from_cookie_str("other_cookie=12345"), None);
    assert_eq!(parse_session_id_from_cookie_str(""), None);

    let mut headers = axum::http::HeaderMap::new();
    assert_eq!(extract_session_id_from_cookie(&headers), None);
    headers.insert(
        axum::http::header::COOKIE,
        "session_id=424242".parse().unwrap(),
    );
    assert_eq!(extract_session_id_from_cookie(&headers), Some(424242));
}

// spec: alhalte-maitre-d/room-permissions-sqlite
#[test]
fn test_sqlite_room_permissions() {
    let db = Database::new_in_memory().unwrap();
    let acc1 = db
        .get_or_create_account("sub-1", "Alice", Some([0x11; 32]), 1000)
        .unwrap();
    let acc2 = db
        .get_or_create_account("sub-2", "Bob", None, 1000)
        .unwrap();
    let acc3 = db
        .get_or_create_account("sub-3", "Charlie", None, 1000)
        .unwrap();

    let room_id = "test-room-1";

    // Initially no permissions
    assert!(!db.has_room_permissions(room_id).unwrap());
    assert_eq!(db.get_room_permissions(room_id).unwrap().len(), 0);
    assert_eq!(db.get_room_role(room_id, acc1.account_id).unwrap(), None);

    // Set GM for Alice
    db.set_room_permission(room_id, acc1.account_id, RoomRole::Gm, 1000)
        .unwrap();
    assert!(db.has_room_permissions(room_id).unwrap());
    assert_eq!(
        db.get_room_role(room_id, acc1.account_id).unwrap(),
        Some(RoomRole::Gm)
    );
    assert_eq!(db.get_room_gm(room_id).unwrap(), Some(acc1.account_id));

    // Set PL for Bob and Viewer for Charlie
    db.set_room_permission(room_id, acc2.account_id, RoomRole::Pl, 1010)
        .unwrap();
    db.set_room_permission(room_id, acc3.account_id, RoomRole::Viewer, 1020)
        .unwrap();

    let perms = db.get_room_permissions(room_id).unwrap();
    assert_eq!(perms.len(), 3);
    assert_eq!(perms[0].account_id, acc1.account_id);
    assert_eq!(perms[0].username, "Alice");
    assert_eq!(
        perms[0].icon_url,
        Some(format!("/api/cellar/tin/{}", hex::encode([0x11; 32])))
    );
    assert_eq!(perms[0].role, RoomRole::Gm);

    assert_eq!(perms[1].account_id, acc2.account_id);
    assert_eq!(perms[1].username, "Bob");
    assert_eq!(perms[1].icon_url, None);
    assert_eq!(perms[1].role, RoomRole::Pl);

    assert_eq!(perms[2].account_id, acc3.account_id);
    assert_eq!(perms[2].role, RoomRole::Viewer);

    // Update Bob's role to Viewer
    db.set_room_permission(room_id, acc2.account_id, RoomRole::Viewer, 1030)
        .unwrap();
    assert_eq!(
        db.get_room_role(room_id, acc2.account_id).unwrap(),
        Some(RoomRole::Viewer)
    );

    // Delete Charlie's permission
    assert!(db.delete_room_permission(room_id, acc3.account_id).unwrap());
    assert_eq!(db.get_room_role(room_id, acc3.account_id).unwrap(), None);
    let perms_after = db.get_room_permissions(room_id).unwrap();
    assert_eq!(perms_after.len(), 2);
}

// spec: alhalte-maitre-d/accounts-search-sqlite
#[test]
fn test_sqlite_account_search() {
    let db = Database::new_in_memory().unwrap();
    let acc1 = db
        .get_or_create_account("sub-1", "Alice Wonder", None, 1000)
        .unwrap();
    let acc2 = db
        .get_or_create_account("sub-2", "Bob Builder", None, 1000)
        .unwrap();
    let acc3 = db
        .get_or_create_account("sub-3", "Alice Cooper", None, 1000)
        .unwrap();

    // Substring search for "Alice"
    let results = db.search_accounts("Alice").unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].account_id, acc1.account_id);
    assert_eq!(results[1].account_id, acc3.account_id);

    // Numeric search by ID
    let results_id = db.search_accounts(&acc2.account_id.to_string()).unwrap();
    assert!(!results_id.is_empty());
    assert_eq!(results_id[0].account_id, acc2.account_id);

    // No match
    let results_empty = db.search_accounts("NonExistent").unwrap();
    assert!(results_empty.is_empty());
}

// spec: alhalte-maitre-d/accounts-search-api
#[tokio::test]
async fn test_accounts_search_endpoint() {
    let db = Database::new_in_memory().unwrap();
    let icon_bytes = [0x22u8; 32];
    let acc1 = db
        .get_or_create_account("sub-search-1", "Shinji", Some(icon_bytes), 1000)
        .unwrap();
    let acc2 = db
        .get_or_create_account("sub-search-2", "Asuka", None, 1000)
        .unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        "/tmp", 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Search by name "Shinji"
    let resp = client
        .get(format!("http://{addr}/api/accounts/search?q=Shinji"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let items: Vec<AccountSearchItem> = resp.json().await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].account_id, acc1.account_id);
    assert_eq!(items[0].username, "Shinji");
    assert_eq!(
        items[0].icon_url,
        Some(format!("/api/cellar/tin/{}", hex::encode(icon_bytes)))
    );

    // 2. Search by ID
    let resp_id = client
        .get(format!(
            "http://{addr}/api/accounts/search?q={}",
            acc2.account_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_id.status(), reqwest::StatusCode::OK);
    let items_id: Vec<AccountSearchItem> = resp_id.json().await.unwrap();
    assert_eq!(items_id.len(), 1);
    assert_eq!(items_id[0].account_id, acc2.account_id);
    assert_eq!(items_id[0].username, "Asuka");
    assert_eq!(items_id[0].icon_url, None);

    // 3. Empty query returns empty array
    let resp_empty = client
        .get(format!("http://{addr}/api/accounts/search?q="))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_empty.status(), reqwest::StatusCode::OK);
    let items_empty: Vec<AccountSearchItem> = resp_empty.json().await.unwrap();
    assert!(items_empty.is_empty());
}

// spec: alhalte-maitre-d/room-permissions-api
#[tokio::test]
async fn test_room_permissions_rest_api() {
    let db = Database::new_in_memory().unwrap();
    let acc1 = db
        .get_or_create_account("sub-perm-1", "GM_Player", None, 1000)
        .unwrap();
    let acc2 = db
        .get_or_create_account("sub-perm-2", "Normal_Player", None, 1000)
        .unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        "/tmp", 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let room_id = "test-perm-room";

    // 1. Initial GET /api/room/{room_id}/permissions returns empty list
    let get_initial = client
        .get(format!("http://{addr}/api/room/{room_id}/permissions"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_initial.status(), reqwest::StatusCode::OK);
    let perms: Vec<RoomPermissionItem> = get_initial.json().await.unwrap();
    assert!(perms.is_empty());

    // 2. POST /api/room/{room_id}/permissions to add GM
    let post_gm = client
        .post(format!("http://{addr}/api/room/{room_id}/permissions"))
        .json(&SetRoomPermissionRequest {
            account_id: acc1.account_id,
            role: RoomRole::Gm,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(post_gm.status(), reqwest::StatusCode::OK);

    // 3. POST /api/room/{room_id}/permissions to add Player
    let post_pl = client
        .post(format!("http://{addr}/api/room/{room_id}/permissions"))
        .json(&SetRoomPermissionRequest {
            account_id: acc2.account_id,
            role: RoomRole::Pl,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(post_pl.status(), reqwest::StatusCode::OK);

    // 4. POST with non-existent account returns 404
    let post_unknown = client
        .post(format!("http://{addr}/api/room/{room_id}/permissions"))
        .json(&serde_json::json!({
            "account_id": 99999,
            "role": "viewer"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(post_unknown.status(), reqwest::StatusCode::NOT_FOUND);

    // 5. GET /api/room/{room_id}/permissions verifies both permissions
    let get_resp = client
        .get(format!("http://{addr}/api/room/{room_id}/permissions"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), reqwest::StatusCode::OK);
    let perms_list: Vec<RoomPermissionItem> = get_resp.json().await.unwrap();
    assert_eq!(perms_list.len(), 2);
    assert_eq!(perms_list[0].account_id, acc1.account_id);
    assert_eq!(perms_list[0].role, RoomRole::Gm);
    assert_eq!(perms_list[1].account_id, acc2.account_id);
    assert_eq!(perms_list[1].role, RoomRole::Pl);

    // 6. DELETE /api/room/{room_id}/permissions/{account_id}
    let del_resp = client
        .delete(format!(
            "http://{addr}/api/room/{room_id}/permissions/{}",
            acc2.account_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), reqwest::StatusCode::OK);

    // 7. Verify deletion
    let get_after_del = client
        .get(format!("http://{addr}/api/room/{room_id}/permissions"))
        .send()
        .await
        .unwrap();
    let perms_after_del: Vec<RoomPermissionItem> = get_after_del.json().await.unwrap();
    assert_eq!(perms_after_del.len(), 1);
    assert_eq!(perms_after_del[0].account_id, acc1.account_id);
}

// spec: alhalte-maitre-d/websocket-auth-and-done-rejection
#[tokio::test]
async fn test_websocket_unauthorized_room_connection_rejected_and_done_permitted() {
    let temp_dir = tempdir().unwrap();
    let socket_dir = temp_dir.path().to_path_buf();

    let db = Database::new_in_memory().unwrap();
    let gm = db
        .get_or_create_account("sub-gm", "GM", None, 1000)
        .unwrap();
    let gm_sess = db.create_session(gm.account_id, 1000).unwrap();

    let outsider = db
        .get_or_create_account("sub-out", "Outsider", None, 1000)
        .unwrap();
    let outsider_sess = db.create_session(outsider.account_id, 1000).unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager.clone());
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 901;

    // Set permission for GM on room 901
    db.set_room_permission(&room_id.to_string(), gm.account_id, RoomRole::Gm, 1000)
        .unwrap();

    // 1. Outsider attempts to connect to private room -> Rejected with 403 Forbidden!
    let outsider_ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        outsider.account_id, outsider_sess.session_id
    );
    match connect_async(&outsider_ws_url).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
            assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
        }
        other => panic!("Expected 403 Forbidden rejection, got: {other:?}"),
    }

    // 2. Unauthenticated user attempts to connect -> Rejected with 403 Forbidden!
    let anon_ws_url = format!("ws://{addr}/ws/room/{room_id}");
    match connect_async(&anon_ws_url).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
            assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
        }
        other => panic!("Expected 403 Forbidden rejection, got: {other:?}"),
    }

    // 3. GM connects successfully
    let gm_ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        gm.account_id, gm_sess.session_id
    );
    let (mut gm_stream, _) = connect_async(&gm_ws_url).await.unwrap();

    // 4. GM marks the room done (public)
    let set_done_frame = serde_json::to_string(&ClientFrame::SetDone { done: true }).unwrap();
    gm_stream
        .send(WsMessage::Text(set_done_frame.into()))
        .await
        .unwrap();

    // Give a brief moment for set_done processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 5. Now outsider attempts to connect again -> succeeds because room is done!
    let (mut outsider_stream, _) = connect_async(&outsider_ws_url).await.unwrap();
    let list_scenes = serde_json::to_string(&ClientFrame::ListScenes).unwrap();
    outsider_stream
        .send(WsMessage::Text(list_scenes.into()))
        .await
        .unwrap();

    let mut scene_received = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), outsider_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::SceneList { scenes }) = serde_json::from_str::<ServerFrame>(&t)
        {
            assert!(scenes.contains(&"/".to_string()));
            scene_received = true;
            break;
        }
    }
    assert!(scene_received);

    gm_stream.close(None).await.unwrap();
    outsider_stream.close(None).await.unwrap();
}

// spec: alhalte-maitre-d/websocket-cookie-session
#[tokio::test]
async fn test_websocket_cookie_session_authentication() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let temp_dir = tempdir().unwrap();
    let socket_dir = temp_dir.path().to_path_buf();

    let db = Database::new_in_memory().unwrap();
    let user = db
        .get_or_create_account("sub-cookie", "CookieUser", None, 1000)
        .unwrap();
    let session = db.create_session(user.account_id, 1000).unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 902;

    // Construct WebSocket handshake request with Cookie header
    let ws_url = format!("ws://{addr}/ws/room/{room_id}");
    let mut req = ws_url.into_client_request().unwrap();
    req.headers_mut().insert(
        axum::http::header::COOKIE,
        format!("session_id={}", session.session_id)
            .parse()
            .unwrap(),
    );

    let (mut ws_stream, _) = connect_async(req).await.unwrap();

    let list_req = serde_json::to_string(&ClientFrame::ListScenes).unwrap();
    ws_stream
        .send(WsMessage::Text(list_req.into()))
        .await
        .unwrap();

    let mut scene_received = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::SceneList { scenes }) = serde_json::from_str::<ServerFrame>(&t)
        {
            assert!(scenes.contains(&"/".to_string()));
            scene_received = true;
            break;
        }
    }
    assert!(
        scene_received,
        "Failed to connect and query scenes using Cookie session"
    );
    ws_stream.close(None).await.unwrap();
}

// spec: alhalte-maitre-d/websocket-viewer-mutate-blocking
#[tokio::test]
async fn test_websocket_viewer_mutate_blocking() {
    let temp_dir = tempdir().unwrap();
    let socket_dir = temp_dir.path().to_path_buf();

    let db = Database::new_in_memory().unwrap();
    let gm = db
        .get_or_create_account("sub-gm-v", "GM", None, 1000)
        .unwrap();
    let gm_sess = db.create_session(gm.account_id, 1000).unwrap();

    let viewer = db
        .get_or_create_account("sub-viewer", "Audience", None, 1000)
        .unwrap();
    let viewer_sess = db.create_session(viewer.account_id, 1000).unwrap();

    let room_id: u64 = 903;
    let room_id_str = room_id.to_string();

    // Register GM and Viewer permissions
    db.set_room_permission(&room_id_str, gm.account_id, RoomRole::Gm, 1000)
        .unwrap();
    db.set_room_permission(&room_id_str, viewer.account_id, RoomRole::Viewer, 1010)
        .unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner);
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // 1. Viewer connects
    let viewer_ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        viewer.account_id, viewer_sess.session_id
    );
    let (mut viewer_stream, _) = connect_async(&viewer_ws_url).await.unwrap();

    // 2. Viewer enters root scene (allowed query / navigation)
    let enter_frame = serde_json::to_string(&ClientFrame::EnterScene {
        scene_id: "/".to_string(),
    })
    .unwrap();
    viewer_stream
        .send(WsMessage::Text(enter_frame.into()))
        .await
        .unwrap();

    // 3. Viewer attempts to AppendLog (MUTATE) -> MUST be blocked and return ServerFrame::Error!
    let append_frame = serde_json::to_string(&ClientFrame::AppendLog {
        scene_id: "/".to_string(),
        entry: Some(tin::Tin::from_string("Hello from spectator")),
    })
    .unwrap();
    viewer_stream
        .send(WsMessage::Text(append_frame.into()))
        .await
        .unwrap();

    let mut error_received = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), viewer_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::Error { code, message }) =
                serde_json::from_str::<ServerFrame>(&t)
        {
            assert_eq!(code, "PermissionDenied");
            assert!(message.contains("閲覧者"));
            error_received = true;
            break;
        }
    }
    assert!(
        error_received,
        "Expected AppendLog from viewer to be blocked with PermissionDenied error"
    );

    // 4. Viewer attempts to UpdateState (MUTATE) -> MUST be blocked
    let update_state_frame = serde_json::to_string(&ClientFrame::UpdateState {
        scene_id: "/".to_string(),
        key: "test::key".to_string(),
        value: tin::Tin::from_string("evil update"),
    })
    .unwrap();
    viewer_stream
        .send(WsMessage::Text(update_state_frame.into()))
        .await
        .unwrap();

    let mut update_error_received = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), viewer_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::Error { code, .. }) = serde_json::from_str::<ServerFrame>(&t)
        {
            assert_eq!(code, "PermissionDenied");
            update_error_received = true;
            break;
        }
    }
    assert!(
        update_error_received,
        "Expected UpdateState from viewer to be blocked"
    );

    // 5. Viewer attempts to DeleteState (MUTATE) -> MUST be blocked
    let del_state_frame = serde_json::to_string(&ClientFrame::DeleteState {
        scene_id: "/".to_string(),
        key: "test::key".to_string(),
    })
    .unwrap();
    viewer_stream
        .send(WsMessage::Text(del_state_frame.into()))
        .await
        .unwrap();

    let mut del_error_received = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), viewer_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::Error { code, .. }) = serde_json::from_str::<ServerFrame>(&t)
        {
            assert_eq!(code, "PermissionDenied");
            del_error_received = true;
            break;
        }
    }
    assert!(
        del_error_received,
        "Expected DeleteState from viewer to be blocked"
    );

    // 6. GM connects and mutates state -> succeeds, and viewer receives the broadcast!
    let gm_ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        gm.account_id, gm_sess.session_id
    );
    let (mut gm_stream, _) = connect_async(&gm_ws_url).await.unwrap();

    // GM enters scene
    let gm_enter = serde_json::to_string(&ClientFrame::EnterScene {
        scene_id: "/".to_string(),
    })
    .unwrap();
    gm_stream
        .send(WsMessage::Text(gm_enter.into()))
        .await
        .unwrap();

    // GM updates state
    let gm_update = serde_json::to_string(&ClientFrame::UpdateState {
        scene_id: "/".to_string(),
        key: "system::title".to_string(),
        value: tin::Tin::from_string("Campaign One"),
    })
    .unwrap();
    gm_stream
        .send(WsMessage::Text(gm_update.into()))
        .await
        .unwrap();

    // Viewer should receive StateUpdated broadcast!
    let mut viewer_received_broadcast = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), viewer_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::StateUpdated { scene_id, key, .. }) =
                serde_json::from_str::<ServerFrame>(&t)
            && scene_id == "/"
            && key == "system::title"
        {
            viewer_received_broadcast = true;
            break;
        }
    }
    assert!(
        viewer_received_broadcast,
        "Viewer should receive broadcast of StateUpdated from GM"
    );

    viewer_stream.close(None).await.unwrap();
    gm_stream.close(None).await.unwrap();
}
