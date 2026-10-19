//! Comprehensive test suite for `alhalte-maitre-d`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, TimeZone};
use futures_util::{SinkExt, StreamExt};
use tempfile::tempdir;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use alhalte_banquette::{ClientFrame, RoomPermissionLevel, ServerFrame};
use alhalte_maitre_d::{
    AccountSearchItem, AppState, BanquetteSpawner, DEBUG_SUB_CIPHERTEXT, Database,
    IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID, IDENT_ALHALTE_MAITRE_D_SESSION_ID,
    InProcessBanquetteSpawner, LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID,
    LABEL_ALHALTE_MAITRE_D_SESSION_ID, MockOAuthProvider, RoomBridgeManager, RoomInfoResponse,
    RoomPermissionItem, RoomRole, SetRoomPermissionRequest, account_id_from_tin, account_id_to_tin,
    cleanup_sessions_at, cleanup_sessions_with_cutoff, create_router, decrypt_sub, encrypt_sub,
    extract_session_id_from_cookie, parse_cli_args_from, parse_session_id_from_cookie_str,
    previous_4am_local, resolve_banquette_bin, session_id_from_tin, session_id_to_tin,
};

// spec: alhalte-maitre-d/rooms registry
/// Registers `room_id` in the room registry with `creator` as its creator.
///
/// The creator becomes the room's initial `Admin` when banquette starts the
/// room; `maitre-d` writes no permission row.
fn register_room(db: &Database, room_id: u64, creator: u64, name: &str) {
    db.create_room(&format!("{room_id:016x}"), name, creator, 1000)
        .unwrap();
}

// spec: alhalte-maitre-d/room-head 0001 room_heads tin pointer round trip
#[test]
fn test_room_head_round_trip_and_overwrite() {
    let db = Database::new_in_memory().unwrap();
    let room_id = "00000000000000aa";
    assert_eq!(db.get_room_head(room_id).unwrap(), None);

    let first = [0x01u8; 32];
    assert_eq!(db.set_room_head(room_id, &first, 1000).unwrap(), None);
    assert_eq!(db.get_room_head(room_id).unwrap(), Some(first));

    let second = [0x02u8; 32];
    assert_eq!(
        db.set_room_head(room_id, &second, 2000).unwrap(),
        Some(first),
        "the superseded head is reported for retention"
    );
    assert_eq!(db.get_room_head(room_id).unwrap(), Some(second));

    // Heads are per room.
    assert_eq!(db.get_room_head("00000000000000ab").unwrap(), None);
}

type SpawnCalls = Arc<std::sync::Mutex<Vec<(u64, u64, Option<[u8; 32]>)>>>;

/// Wraps an in-process spawner and records every `(room_id, admin_account, room_head)`.
#[derive(Clone)]
struct RecordingSpawner {
    inner: Arc<InProcessBanquetteSpawner>,
    calls: SpawnCalls,
}

impl RecordingSpawner {
    fn new(socket_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            inner: Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
                socket_dir, 8,
            )),
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<(u64, u64, Option<[u8; 32]>)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl BanquetteSpawner for RecordingSpawner {
    async fn spawn_banquette(
        &self,
        room_id: u64,
        admin_account: u64,
        room_head: Option<[u8; 32]>,
    ) -> alhalte_maitre_d::Result<std::path::PathBuf> {
        self.calls
            .lock()
            .unwrap()
            .push((room_id, admin_account, room_head));
        self.inner
            .spawn_banquette(room_id, admin_account, room_head)
            .await
    }
}

/// Spawns a router backed by an in-memory DB and a [`RecordingSpawner`].
async fn spawn_recording_router() -> (
    std::net::SocketAddr,
    tempfile::TempDir,
    Database,
    RecordingSpawner,
    RoomBridgeManager,
) {
    let temp_dir = tempdir().unwrap();
    let spawner = RecordingSpawner::new(temp_dir.path().to_path_buf());
    let db = Database::new_in_memory().unwrap();
    let manager = RoomBridgeManager::new(Arc::new(spawner.clone()), db.clone());
    let state = AppState::new(
        db.clone(),
        "salt",
        Arc::new(MockOAuthProvider::new()),
        manager.clone(),
    );
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, temp_dir, db, spawner, manager)
}

/// Creates an account and an active session for room API tests.
fn account_with_session(db: &Database, sub: &str, name: &str) -> (u64, u64) {
    let account = db.get_or_create_account(sub, name, None, 1000).unwrap();
    let session = db.create_session(account.account_id, 1000).unwrap();
    (account.account_id, session.session_id)
}

/// Asserts that a WebSocket handshake is rejected with `expected`.
async fn expect_ws_rejection(url: String, expected: axum::http::StatusCode) {
    match connect_async(url).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), expected);
        }
        Ok((mut ws, _)) => {
            let _ = ws.close(None).await;
            panic!("expected HTTP {expected:?} but the WebSocket handshake succeeded");
        }
        other => panic!("expected HTTP {expected:?}, got {other:?}"),
    }
}

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
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
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
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
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
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
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
async fn test_oauth_login_and_callback_flow() {
    let db = Database::new_in_memory().unwrap();
    let mock_oauth = Arc::new(MockOAuthProvider::new());
    mock_oauth.register_code("auth-code-123", "google-sub-frank");
    mock_oauth.register_token("id-token-abc", "google-sub-frank");

    let spawner =
        Arc::new(alhalte_maitre_d::InProcessBanquetteSpawner::new_with_in_memory_cellar("/tmp", 8));
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let salt = "my-test-salt";
    let state = AppState::new(db.clone(), salt, mock_oauth, room_manager)
        .with_public_url("http://maitre-d.test/")
        .with_allowed_origins(["http://app.test"]);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // OAuth is a browser navigation flow: never follow redirects in tests.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    // spec: alhalte-maitre-d/oauth-return-to 0001 disallowed return_to origins are rejected
    for bad_return_to in [
        "http://evil.example/callback",
        "//evil.example",
        "javascript:alert(1)",
        "http://app.test@evil.example/",
        "/relative",
        "",
    ] {
        let resp = client
            .get(format!("http://{addr}/auth/login"))
            .query(&[("return_to", bad_return_to)])
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "return_to {bad_return_to:?} must be rejected"
        );
    }

    // 1. `/auth/login` redirects to Google with the fixed maitre-d redirect URI.
    let login = client
        .get(format!("http://{addr}/auth/login"))
        .query(&[
            ("return_to", "http://app.test/rooms/abc?invite=1"),
            ("prompt", "select_account"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), reqwest::StatusCode::FOUND);

    let location = login
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let auth_url = reqwest::Url::parse(&location).unwrap();
    assert_eq!(auth_url.host_str(), Some("mock-google-oauth.example.com"));
    let params: std::collections::HashMap<String, String> =
        auth_url.query_pairs().into_owned().collect();
    // The argenterie origin / port is never sent to Google.
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some("http://maitre-d.test/auth/google/callback")
    );
    assert_eq!(
        params.get("prompt").map(String::as_str),
        Some("select_account")
    );
    let oauth_state = params.get("state").cloned().expect("state must be issued");
    assert_eq!(oauth_state.len(), 64, "state is 32 random bytes as hex");

    // 2. The callback consumes the state, creates the session, sets the cookie,
    //    and redirects to the absolute return_to.
    let callback = client
        .get(format!("http://{addr}/auth/google/callback"))
        .query(&[("code", "auth-code-123"), ("state", oauth_state.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), reqwest::StatusCode::FOUND);
    assert_eq!(
        callback
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "http://app.test/rooms/abc?invite=1"
    );
    let set_cookie = callback
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .find_map(|value| value.to_str().ok())
        .expect("callback must set the session cookie");
    assert!(set_cookie.contains("session_id="));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(
        !set_cookie.contains("Secure"),
        "http public URL has no Secure"
    );
    let session_id = set_cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("session_id=")
        .parse::<u64>()
        .unwrap();

    // 3. `/api/session` resolves the account from the cookie for argenterie.
    let session_resp = client
        .get(format!("http://{addr}/api/session"))
        .header(reqwest::header::COOKIE, format!("session_id={session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(session_resp.status(), reqwest::StatusCode::OK);
    let session_data: alhalte_maitre_d::SessionResponse = session_resp.json().await.unwrap();
    assert_eq!(session_data.session_id, session_id);
    assert_eq!(session_data.account.handle_name, "Anonymous");

    // The sub is encrypted with the instance salt and never exposed by /api/session.
    let stored = db
        .get_account(session_data.account.account_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.encrypted_sub,
        encrypt_sub(salt, "google-sub-frank").unwrap()
    );
    assert_eq!(
        decrypt_sub(salt, &stored.encrypted_sub).unwrap(),
        "google-sub-frank"
    );

    // spec: alhalte-maitre-d/oauth-state 0001 states are single-use
    let replay = client
        .get(format!("http://{addr}/auth/google/callback"))
        .query(&[("code", "auth-code-123"), ("state", oauth_state.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), reqwest::StatusCode::BAD_REQUEST);

    // 4. `/auth/google/token` still issues a fresh session for the same account.
    let token_resp = client
        .post(format!("http://{addr}/auth/google/token"))
        .json(&serde_json::json!({ "id_token": "id-token-abc" }))
        .send()
        .await
        .unwrap();
    assert_eq!(token_resp.status(), reqwest::StatusCode::OK);
    let auth_data2: alhalte_maitre_d::AuthResponse = token_resp.json().await.unwrap();
    assert_eq!(
        auth_data2.account.account_id,
        session_data.account.account_id
    );
    assert_ne!(auth_data2.session_id, session_id);
    assert!(
        db.is_session_valid(auth_data2.account.account_id, auth_data2.session_id)
            .unwrap()
    );

    // 5. The old client-driven endpoints are gone.
    let old_url = client
        .get(format!(
            "http://{addr}/auth/google/url?redirect_uri=http://app.test/&state=xyz"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(old_url.status(), reqwest::StatusCode::NOT_FOUND);
    let old_post = client
        .post(format!("http://{addr}/auth/google/callback"))
        .json(&serde_json::json!({
            "code": "auth-code-123",
            "redirect_uri": "http://app.test/",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(old_post.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
}

// spec: alhalte-maitre-d/oauth-state 0002 expired states are rejected
#[test]
fn test_oauth_state_single_use_and_expiry() {
    let db = Database::new_in_memory().unwrap();
    let ttl = alhalte_maitre_d::OAUTH_STATE_TTL_SECS;

    db.create_oauth_state("state-1", "http://app.test/", 1000, ttl)
        .unwrap();
    assert_eq!(
        db.consume_oauth_state("state-1", 1000, ttl).unwrap(),
        Some("http://app.test/".to_string())
    );
    assert_eq!(db.consume_oauth_state("state-1", 1000, ttl).unwrap(), None);

    db.create_oauth_state("state-2", "http://app.test/", 1000, ttl)
        .unwrap();
    assert_eq!(
        db.consume_oauth_state("state-2", 1000 + ttl + 1, ttl)
            .unwrap(),
        None
    );

    // Unknown states are rejected without leaking whether they existed.
    assert_eq!(
        db.consume_oauth_state("never-issued", 1000, ttl).unwrap(),
        None
    );
}

// spec: alhalte-maitre-d/oauth-return-to 0002 open redirects are rejected
#[test]
fn test_oauth_return_to_origin_validation() {
    let allowed = vec![
        "http://127.0.0.1:5173".to_string(),
        "https://app.example".to_string(),
    ];

    assert!(alhalte_maitre_d::is_allowed_return_to(
        "http://127.0.0.1:5173/rooms/1?invite=1#room-key",
        &allowed
    ));
    assert!(alhalte_maitre_d::is_allowed_return_to(
        "https://app.example",
        &allowed
    ));
    // Default ports normalize to the bare origin.
    assert!(alhalte_maitre_d::is_allowed_return_to(
        "https://app.example:443/rooms/1",
        &allowed
    ));

    for rejected in [
        "http://127.0.0.1:5174/",
        "http://127.0.0.1:5173.evil.example/",
        "//evil.example/",
        "/relative/path",
        "javascript:alert(1)",
        "data:text/html,hi",
        "http://app.example@evil.example/",
        "https://app.example.evil.example/",
        "",
    ] {
        assert!(
            !alhalte_maitre_d::is_allowed_return_to(rejected, &allowed),
            "{rejected:?} must not be accepted as return_to"
        );
    }

    assert!(!alhalte_maitre_d::is_allowed_return_to(
        "https://app.example/",
        &[]
    ));
}

// spec: alhalte-maitre-d/oauth-cookie
#[test]
fn test_session_cookie_header_attributes() {
    assert_eq!(
        alhalte_maitre_d::session_cookie_header(42, false, 60),
        "session_id=42; Path=/; Max-Age=60; SameSite=Lax"
    );
    assert_eq!(
        alhalte_maitre_d::session_cookie_header(42, true, 60),
        "session_id=42; Path=/; Max-Age=60; SameSite=Lax; Secure"
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
        "--public-url",
        "https://auth.example.com",
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
    assert_eq!(parsed.public_url, "https://auth.example.com");
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
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 777;
    register_room(&db, room_id, acc.account_id, "Demux room");

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
            assert!(scenes.iter().any(|s| s.id == "/"));
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
        name: None,
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
            && scenes.iter().any(|s| s.id == "tavern")
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
        socket_dir, 5,
    ));
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // A generated-style 16-hex room ID (contains hex letters).
    let room_id = "9f3a7c1e5b2d8046";
    let room_id_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Export room", acc.account_id, 1000)
        .unwrap();
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
        name: None,
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
    assert_eq!(archive.room_id, Some(room_id_u64));
    // The spawner's chunk size exponent is propagated to banquette.
    assert_eq!(archive.chunk_exponent, 5);
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
    assert_eq!(logs_archive.room_id, Some(room_id_u64));
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

    // Test 6: CORS GET request with a configured Origin receives the explicit
    // allow-origin (never `*` with credentials) and the ZIP binary.
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
        "http://localhost:5173"
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
        "http://localhost:5173"
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

// spec: alhalte-maitre-d/permissions 0001 maitre-d keeps no permission database
//
// The `Database` type has no permission accessor at all (compile time); this
// test pins the SQLite schema: no `room_permissions` table and no `rooms.done`
// column survive a fresh open.
#[test]
fn test_maitre_d_database_has_no_permission_schema() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("schema.sqlite3");
    drop(Database::open(&path).unwrap());

    let conn = rusqlite::Connection::open(&path).unwrap();
    let permission_table: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'room_permissions'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        permission_table, 0,
        "maitre-d must not own a room_permissions table (banquette holds the permission table)"
    );

    let done_column: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('rooms') WHERE name = 'done'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        done_column, 0,
        "maitre-d must not keep a rooms.done projection"
    );

    let room_columns: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info('rooms') ORDER BY cid")
            .unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
        rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    assert_eq!(
        room_columns,
        vec![
            "room_id".to_string(),
            "name".to_string(),
            "creator_account".to_string(),
            "created_at".to_string(),
        ],
        "rooms is a pure registry"
    );
}

// spec: alhalte-maitre-d/permissions 0002 legacy permission rows fail closed
//
// There is no migration: an operator that still has the pre-#200 permission
// model in SQLite must reset the database.
#[test]
fn test_legacy_room_permissions_row_fails_startup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy-permissions.sqlite3");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE room_permissions (
                 room_id TEXT NOT NULL,
                 account_id INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 PRIMARY KEY (room_id, account_id)
             );
             INSERT INTO room_permissions (room_id, account_id, role, created_at)
             VALUES ('0123456789abcdef', 1, 'gm', 1000);",
        )
        .unwrap();
    }

    match Database::open(&path) {
        Err(alhalte_maitre_d::MaitreDError::LegacyRoomPermissions(rows)) => assert_eq!(rows, 1),
        other => panic!("expected LegacyRoomPermissions, got {:?}", other.err()),
    }
}

// spec: alhalte-maitre-d/permissions 0003 legacy rooms.done rows fail closed
#[test]
fn test_legacy_rooms_done_row_fails_startup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy-done.sqlite3");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE rooms (
                 room_id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 creator_account INTEGER NOT NULL,
                 created_at INTEGER NOT NULL,
                 done INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO rooms (room_id, name, creator_account, created_at, done)
             VALUES ('0123456789abcdef', 'Public room', 1, 1000, 1);",
        )
        .unwrap();
    }

    match Database::open(&path) {
        Err(alhalte_maitre_d::MaitreDError::LegacyRoomDone(rooms)) => assert_eq!(rooms, 1),
        other => panic!("expected LegacyRoomDone, got {:?}", other.err()),
    }
}

// spec: alhalte-maitre-d/permissions 0004 empty legacy artifacts are dropped
#[test]
fn test_empty_legacy_permission_artifacts_are_dropped() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty-legacy.sqlite3");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE rooms (
                 room_id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 creator_account INTEGER NOT NULL,
                 created_at INTEGER NOT NULL,
                 done INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO rooms (room_id, name, creator_account, created_at, done)
             VALUES ('0123456789abcdef', 'Private room', 1, 1000, 0);
             CREATE TABLE room_permissions (
                 room_id TEXT NOT NULL,
                 account_id INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 PRIMARY KEY (room_id, account_id)
             );",
        )
        .unwrap();
    }

    // An empty legacy schema carries no information, so it is dropped.
    let db = Database::open(&path).unwrap();
    let room = db.get_room("0123456789abcdef").unwrap().unwrap();
    assert_eq!(room.creator_account, 1);
    assert_eq!(room.name, "Private room");

    let conn = rusqlite::Connection::open(&path).unwrap();
    let permission_table: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'room_permissions'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(permission_table, 0);
    let done_column: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('rooms') WHERE name = 'done'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(done_column, 0);

    // Re-opening is idempotent.
    drop(db);
    Database::open(&path).unwrap();
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

// spec: alhalte-maitre-d/accounts-search-api requires a valid session
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
    let session = db.create_session(acc1.account_id, 1000).unwrap();
    let auth = format!(
        "account_id={}&session_id={}",
        acc1.account_id, session.session_id
    );

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        "/tmp", 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db, "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();

    // Anonymous search is rejected.
    let anonymous = client
        .get(format!("http://{addr}/api/accounts/search?q=Shinji"))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 1. Search by name "Shinji"
    let resp = client
        .get(format!("http://{addr}/api/accounts/search?q=Shinji&{auth}"))
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
            "http://{addr}/api/accounts/search?q={}&{auth}",
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
        .get(format!("http://{addr}/api/accounts/search?q=&{auth}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_empty.status(), reqwest::StatusCode::OK);
    let items_empty: Vec<AccountSearchItem> = resp_empty.json().await.unwrap();
    assert!(items_empty.is_empty());
}

// spec: alhalte-maitre-d/permissions 0010 the HTTP permission API is a banquette proxy
//
// The routes stay for the pre-#201 argenterie build, but they neither read nor
// write a maitre-d permission table: `GET` relays `QueryPermissions` and
// `POST` / `DELETE` relay `SetMemberPermission`. banquette authorizes every
// change, and its `RoomData` is the state that changes.
#[tokio::test]
async fn test_room_permissions_rest_api_proxies_banquette() {
    let (addr, _temp_dir, db, _spawner, manager) = spawn_recording_router().await;
    let (gm, gm_session) = account_with_session(&db, "sub-perm-1", "GM_Player");
    let (player, player_session) = account_with_session(&db, "sub-perm-2", "Normal_Player");
    let (outsider, outsider_session) = account_with_session(&db, "sub-perm-3", "Outsider");
    let room_id = "0123456789abcdef";
    let room_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Permissions room", gm, 1000)
        .unwrap();

    let client = reqwest::Client::new();
    let gm_auth = format!("account_id={gm}&session_id={gm_session}");
    let player_auth = format!("account_id={player}&session_id={player_session}");
    let outsider_auth = format!("account_id={outsider}&session_id={outsider_session}");

    // 1. A session is required, and a denied account is rejected.
    let get_anonymous = client
        .get(format!("http://{addr}/api/room/{room_id}/permissions"))
        .send()
        .await
        .unwrap();
    assert_eq!(get_anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    let get_outsider_private = client
        .get(format!(
            "http://{addr}/api/room/{room_id}/permissions?{outsider_auth}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        get_outsider_private.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    // 2. The creator is banquette's initial Admin, so the snapshot lists them.
    let get_admin = client
        .get(format!(
            "http://{addr}/api/room/{room_id}/permissions?{gm_auth}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(get_admin.status(), reqwest::StatusCode::OK);
    let perms: Vec<RoomPermissionItem> = get_admin.json().await.unwrap();
    assert_eq!(perms.len(), 1);
    assert_eq!(perms[0].account_id, gm);
    assert_eq!(perms[0].username, "GM_Player");
    assert_eq!(perms[0].role, RoomRole::Admin);

    // 3. A non-existent target is 404; a denied caller cannot change anything.
    let post_unknown = client
        .post(format!(
            "http://{addr}/api/room/{room_id}/permissions?{gm_auth}"
        ))
        .json(&serde_json::json!({ "account_id": 99999, "role": "viewer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(post_unknown.status(), reqwest::StatusCode::NOT_FOUND);

    let post_anonymous = client
        .post(format!("http://{addr}/api/room/{room_id}/permissions"))
        .json(&serde_json::json!({ "account_id": player, "role": "viewer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(post_anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 4. The Admin adds a participant. banquette's `RoomData` changes; the
    //    maitre-d database is not touched (it has nowhere to store it).
    let post_player = client
        .post(format!(
            "http://{addr}/api/room/{room_id}/permissions?{gm_auth}"
        ))
        .json(&SetRoomPermissionRequest {
            account_id: player,
            role: RoomRole::Participant,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(post_player.status(), reqwest::StatusCode::OK);

    let bridge = manager
        .get_or_create_room(room_u64, gm)
        .await
        .expect("the room process is reusable");
    assert_eq!(
        bridge.query_account_permission(player).await.unwrap(),
        RoomPermissionLevel::Participant,
        "the HTTP proxy must change banquette's permission table"
    );

    // 5. A participant (not an Admin) cannot change permissions: banquette
    //    refuses and the route reports 403, not 500.
    let post_non_admin = client
        .post(format!(
            "http://{addr}/api/room/{room_id}/permissions?{player_auth}"
        ))
        .json(&serde_json::json!({ "account_id": outsider, "role": "viewer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(post_non_admin.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(
        bridge.query_account_permission(outsider).await.unwrap(),
        RoomPermissionLevel::Denied,
        "a refused change must not touch the permission table"
    );

    // 6. DELETE installs an explicit Denied entry (banquette's model has no
    //    "absent" state for an account that must be blocked).
    let del_non_admin = client
        .delete(format!(
            "http://{addr}/api/room/{room_id}/permissions/{outsider}?{player_auth}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(del_non_admin.status(), reqwest::StatusCode::FORBIDDEN);

    let del_admin = client
        .delete(format!(
            "http://{addr}/api/room/{room_id}/permissions/{player}?{gm_auth}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(del_admin.status(), reqwest::StatusCode::OK);
    assert_eq!(
        bridge.query_account_permission(player).await.unwrap(),
        RoomPermissionLevel::Denied
    );

    let get_after_del = client
        .get(format!(
            "http://{addr}/api/room/{room_id}/permissions?{gm_auth}"
        ))
        .send()
        .await
        .unwrap();
    let perms_after_del: Vec<RoomPermissionItem> = get_after_del.json().await.unwrap();
    assert!(
        perms_after_del.iter().any(
            |permission| permission.account_id == player && permission.role == RoomRole::Denied
        ),
        "an explicit denial is visible in the snapshot: {perms_after_del:?}"
    );

    // 7. An unknown room is 404 and never spawns anything.
    let get_unknown = client
        .get(format!(
            "http://{addr}/api/room/ffffffffffffffff/permissions?{gm_auth}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(get_unknown.status(), reqwest::StatusCode::NOT_FOUND);
}

// spec: alhalte-maitre-d/account-api 0001 GET /api/account/{id} requires a session and hides encrypted_sub
#[tokio::test]
async fn test_get_account_response_hides_encrypted_sub_and_requires_session() {
    let (addr, _temp_dir, db, _spawner, _manager) = spawn_recording_router().await;
    let icon_bytes = [0x33u8; 32];
    let account = db
        .get_or_create_account("sub-account-dto", "DtoUser", Some(icon_bytes), 1000)
        .unwrap();
    let session = db.create_session(account.account_id, 1000).unwrap();
    let auth = format!(
        "account_id={}&session_id={}",
        account.account_id, session.session_id
    );
    let client = reqwest::Client::new();

    let anonymous = client
        .get(format!("http://{addr}/api/account/{}", account.account_id))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    let response = client
        .get(format!(
            "http://{addr}/api/account/{}?{auth}",
            account.account_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let value: serde_json::Value = response.json().await.unwrap();
    assert!(
        value.get("encrypted_sub").is_none(),
        "encrypted_sub must never be serialized to clients"
    );
    assert_eq!(value["account_id"], account.account_id);
    assert_eq!(value["handle_name"], "DtoUser");
    assert_eq!(value["icon_tin_id"], serde_json::json!(icon_bytes));

    let unknown = client
        .get(format!("http://{addr}/api/account/99999?{auth}"))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
}

// spec: alhalte-maitre-d/cors 0001 configured origins only, never permissive
#[tokio::test]
async fn test_cors_allows_only_configured_origins() {
    let temp_dir = tempdir().unwrap();
    let spawner = RecordingSpawner::new(temp_dir.path().to_path_buf());
    let db = Database::new_in_memory().unwrap();
    let manager = RoomBridgeManager::new(Arc::new(spawner), db.clone());
    let state = AppState::new(db, "salt", Arc::new(MockOAuthProvider::new()), manager)
        .with_allowed_origins(["http://allowed.example"]);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let preflight = |origin: &'static str| {
        client
            .request(
                reqwest::Method::OPTIONS,
                format!("http://{addr}/api/accounts/search"),
            )
            .header("Origin", origin)
            .header("Access-Control-Request-Method", "GET")
            .send()
    };

    let allowed = preflight("http://allowed.example").await.unwrap();
    assert_eq!(allowed.status(), reqwest::StatusCode::OK);
    assert_eq!(
        allowed
            .headers()
            .get("access-control-allow-origin")
            .and_then(|value| value.to_str().ok()),
        Some("http://allowed.example")
    );

    let denied = preflight("http://evil.example").await.unwrap();
    assert!(
        denied
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "an unconfigured origin must not receive an allow-origin header"
    );

    // The local development origins remain the default allow-list.
    let defaults = alhalte_maitre_d::DEFAULT_ALLOWED_ORIGINS;
    assert!(defaults.contains(&"http://127.0.0.1:5173"));
    assert!(defaults.contains(&"http://localhost:5173"));
}

// spec: alhalte-maitre-d/websocket-identity 0001 account-only 401, session-only derives account
#[tokio::test]
async fn test_websocket_identity_rules() {
    let (addr, _temp_dir, db, _spawner, manager) = spawn_recording_router().await;
    let (account, session) = account_with_session(&db, "sub-ws-identity", "WsUser");
    let room_id = "0123456789abcdfe";
    let room_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Identity room", account, 1000)
        .unwrap();

    // 1. account_id alone is not proof of identity.
    expect_ws_rejection(
        format!("ws://{addr}/ws/room/{room_id}?account_id={account}"),
        axum::http::StatusCode::UNAUTHORIZED,
    )
    .await;

    // 2. An invalid session is rejected, with or without an account claim.
    expect_ws_rejection(
        format!("ws://{addr}/ws/room/{room_id}?account_id={account}&session_id=99999"),
        axum::http::StatusCode::UNAUTHORIZED,
    )
    .await;
    expect_ws_rejection(
        format!("ws://{addr}/ws/room/{room_id}?session_id=99999"),
        axum::http::StatusCode::UNAUTHORIZED,
    )
    .await;

    // 3. An anonymous caller of a room whose default permission is `denied` is
    //    rejected. The room process has to exist to answer that question.
    expect_ws_rejection(
        format!("ws://{addr}/ws/room/{room_id}"),
        axum::http::StatusCode::FORBIDDEN,
    )
    .await;

    // 4. A session alone derives the account and is accepted (the creator is
    //    banquette's initial Admin).
    let (mut session_only, _) = connect_async(format!(
        "ws://{addr}/ws/room/{room_id}?session_id={session}"
    ))
    .await
    .expect("a session-only connection must derive its account");
    let _ = session_only.close(None).await;

    // 5. account_id + a matching valid session is accepted.
    let (mut both, _) = connect_async(format!(
        "ws://{addr}/ws/room/{room_id}?account_id={account}&session_id={session}"
    ))
    .await
    .expect("account_id plus a valid session must be accepted");
    let _ = both.close(None).await;

    // 6. The Admin opens the room to anonymous viewers through banquette's
    //    default permission, and only then is an anonymous connection accepted.
    let bridge = manager
        .get_or_create_room(room_u64, account)
        .await
        .expect("the room process is live");
    bridge
        .set_default_permission(account, RoomPermissionLevel::Viewer)
        .await
        .expect("the Admin may change the default permission");
    let (mut anonymous, _) = connect_async(format!("ws://{addr}/ws/room/{room_id}"))
        .await
        .expect("anonymous must be accepted for a room whose default is viewer");
    let _ = anonymous.close(None).await;
}

// spec: alhalte-maitre-d/rooms registry sqlite
#[test]
fn test_room_registry_create_and_lookup() {
    let db = Database::new_in_memory().unwrap();
    let account = db
        .get_or_create_account("sub-registry", "Registry", None, 1000)
        .unwrap();
    let room_id = "0123456789abcdef";

    assert!(db.get_room(room_id).unwrap().is_none());

    let room = db
        .create_room(room_id, "Crypt", account.account_id, 1000)
        .unwrap();
    assert_eq!(room.room_id, room_id);
    assert_eq!(room.name, "Crypt");
    assert_eq!(room.creator_account, account.account_id);
    assert_eq!(room.created_at, 1000);
    assert_eq!(db.get_room(room_id).unwrap(), Some(room));

    // Registration writes the registry row only: the room's authorization is
    // created by banquette at spawn time (creator = initial Admin). The absence
    // of a permission table is pinned by
    // `test_maitre_d_database_has_no_permission_schema`.

    // Duplicate registration is an error and leaves the original intact.
    let duplicate = db.create_room(room_id, "Other", account.account_id, 2000);
    assert!(matches!(
        duplicate,
        Err(alhalte_maitre_d::MaitreDError::RoomAlreadyExists(_))
    ));
    assert_eq!(db.get_room(room_id).unwrap().unwrap().name, "Crypt");
}

// spec: alhalte-maitre-d/rooms create api
#[tokio::test]
async fn test_create_room_api_registers_creator_as_admin() {
    let (addr, _temp_dir, db, spawner, manager) = spawn_recording_router().await;
    let (account_id, session_id) = account_with_session(&db, "sub-create-room", "Creator");
    let client = reqwest::Client::new();
    let auth = format!("account_id={account_id}&session_id={session_id}");

    // A session is required.
    let unauthenticated = client
        .post(format!("http://{addr}/api/rooms"))
        .json(&serde_json::json!({
            "room_id": "abcdef0123456789",
            "name": "Lair"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);

    let response = client
        .post(format!("http://{addr}/api/rooms?{auth}"))
        .json(&serde_json::json!({
            "room_id": "abcdef0123456789",
            "name": "  Lair  "
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let info: RoomInfoResponse = response.json().await.unwrap();
    assert_eq!(info.room_id, "abcdef0123456789");
    assert_eq!(info.name, "Lair");
    assert_eq!(info.role, RoomRole::Admin, "the creator is the room Admin");

    let room = db.get_room("abcdef0123456789").unwrap().unwrap();
    assert_eq!(room.creator_account, account_id);
    // Creation does not spawn banquette and writes no permission anywhere.
    assert!(spawner.calls().is_empty());

    // The creator really is banquette's initial Admin: the first spawn uses
    // `rooms.creator_account` and `Room::new` inserts it as `Admin`.
    let bridge = manager
        .get_or_create_room(
            u64::from_str_radix("abcdef0123456789", 16).unwrap(),
            account_id,
        )
        .await
        .expect("the room starts");
    assert_eq!(
        bridge.query_account_permission(account_id).await.unwrap(),
        RoomPermissionLevel::Admin
    );

    // Duplicate -> 409.
    let duplicate = client
        .post(format!("http://{addr}/api/rooms?{auth}"))
        .json(&serde_json::json!({
            "room_id": "abcdef0123456789",
            "name": "Duplicate"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), reqwest::StatusCode::CONFLICT);

    // A generated ID is canonical 16-hex and also creates no process.
    let generated = client
        .post(format!("http://{addr}/api/rooms?{auth}"))
        .json(&serde_json::json!({ "name": "Generated" }))
        .send()
        .await
        .unwrap();
    assert_eq!(generated.status(), reqwest::StatusCode::OK);
    let generated_info: RoomInfoResponse = generated.json().await.unwrap();
    assert_eq!(generated_info.room_id.len(), 16);
    assert!(
        generated_info
            .room_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );
    assert_eq!(
        db.get_room(&generated_info.room_id)
            .unwrap()
            .unwrap()
            .creator_account,
        account_id
    );
    // Generating a room ID for an existing creator does not add a process: the
    // only spawn so far is the explicit one above.
    assert_eq!(spawner.calls().len(), 1);
    assert_eq!(
        spawner.calls()[0].0,
        u64::from_str_radix("abcdef0123456789", 16).unwrap()
    );
}

// spec: alhalte-maitre-d/rooms create api validation
#[tokio::test]
async fn test_create_room_api_validates_input() {
    let (addr, _temp_dir, db, _spawner, _manager) = spawn_recording_router().await;
    let (account_id, session_id) = account_with_session(&db, "sub-validate-room", "Creator");
    let client = reqwest::Client::new();
    let auth = format!("account_id={account_id}&session_id={session_id}");

    for invalid_room_id in [
        "room-1",
        "0x1234",
        "zzzzzzzzzzzzzzzz",
        "18446744073709551616",
    ] {
        let response = client
            .post(format!("http://{addr}/api/rooms?{auth}"))
            .json(&serde_json::json!({
                "room_id": invalid_room_id,
                "name": "Valid name"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{invalid_room_id} must be rejected"
        );
    }

    for invalid_name in ["", "   ", "\n\t"] {
        let response = client
            .post(format!("http://{addr}/api/rooms?{auth}"))
            .json(&serde_json::json!({ "name": invalid_name }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "name {invalid_name:?} must be rejected"
        );
    }

    let long_name = "x".repeat(alhalte_maitre_d::MAX_ROOM_NAME_LEN + 1);
    let response = client
        .post(format!("http://{addr}/api/rooms?{auth}"))
        .json(&serde_json::json!({ "name": long_name }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    // Legacy decimal IDs are still accepted and stored/returned canonical.
    let response = client
        .post(format!("http://{addr}/api/rooms?{auth}"))
        .json(&serde_json::json!({ "room_id": "777", "name": "Legacy" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let info: RoomInfoResponse = response.json().await.unwrap();
    assert_eq!(info.room_id, format!("{:016x}", 777));
    assert!(db.get_room("0000000000000309").unwrap().is_some());
}

// spec: alhalte-maitre-d/permissions 0020 GET /api/rooms/{id} answers with the four-level role
#[tokio::test]
async fn test_get_room_info_access_rules() {
    let (addr, _temp_dir, db, spawner, manager) = spawn_recording_router().await;
    let (creator, creator_session) = account_with_session(&db, "sub-info-creator", "Creator");
    let (outsider, outsider_session) = account_with_session(&db, "sub-info-outsider", "Outsider");
    let (viewer, viewer_session) = account_with_session(&db, "sub-info-viewer", "Viewer");
    let room_id = "0123456789abcdef";
    let room_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Info room", creator, 1000).unwrap();
    let client = reqwest::Client::new();

    // Unknown room -> 404, and the registry is not touched / no process starts.
    let unknown = client
        .get(format!(
            "http://{addr}/api/rooms/ffffffffffffffff?account_id={outsider}&session_id={outsider_session}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
    assert!(spawner.calls().is_empty());

    // Authentication is required.
    let anonymous = client
        .get(format!("http://{addr}/api/rooms/{room_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);

    // A caller the room's banquette reports as `denied` gets 403.
    let denied = client
        .get(format!(
            "http://{addr}/api/rooms/{room_id}?account_id={outsider}&session_id={outsider_session}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);

    // The creator is banquette's initial Admin.
    let member = client
        .get(format!(
            "http://{addr}/api/rooms/{room_id}?account_id={creator}&session_id={creator_session}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(member.status(), reqwest::StatusCode::OK);
    let member_info: RoomInfoResponse = member.json().await.unwrap();
    assert_eq!(member_info.role, RoomRole::Admin);
    assert_eq!(member_info.name, "Info room");

    // The room process was started to answer the gate (a permission question
    // cannot be answered from maitre-d's database).
    assert_eq!(spawner.calls(), vec![(room_u64, creator, None)]);

    // An explicit `viewer` / `participant` entry comes back with that role.
    let bridge = manager
        .get_or_create_room(room_u64, creator)
        .await
        .expect("the room process is reusable");
    bridge
        .set_member_permission(creator, viewer, RoomPermissionLevel::Viewer)
        .await
        .unwrap();
    let viewer_response = client
        .get(format!(
            "http://{addr}/api/rooms/{room_id}?account_id={viewer}&session_id={viewer_session}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(viewer_response.status(), reqwest::StatusCode::OK);
    let viewer_info: RoomInfoResponse = viewer_response.json().await.unwrap();
    assert_eq!(viewer_info.role, RoomRole::Viewer);

    bridge
        .set_member_permission(creator, outsider, RoomPermissionLevel::Participant)
        .await
        .unwrap();
    let participant_response = client
        .get(format!(
            "http://{addr}/api/rooms/{room_id}?account_id={outsider}&session_id={outsider_session}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(participant_response.status(), reqwest::StatusCode::OK);
    let participant_info: RoomInfoResponse = participant_response.json().await.unwrap();
    assert_eq!(participant_info.role, RoomRole::Participant);

    // Opening the room default (default permission = viewer) lets an unrelated
    // account, and an anonymous caller, through as `viewer`.
    bridge
        .set_default_permission(creator, RoomPermissionLevel::Viewer)
        .await
        .unwrap();
    let (newcomer, newcomer_session) = account_with_session(&db, "sub-info-newcomer", "Newcomer");
    let public = client
        .get(format!(
            "http://{addr}/api/rooms/{room_id}?account_id={newcomer}&session_id={newcomer_session}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(public.status(), reqwest::StatusCode::OK);
    let public_info: RoomInfoResponse = public.json().await.unwrap();
    assert_eq!(public_info.role, RoomRole::Viewer);
}

// spec: alhalte-maitre-d/permissions 0021 an anonymous default is the room default
#[tokio::test]
async fn test_anonymous_role_follows_the_default_permission() {
    let (addr, _temp_dir, db, _spawner, manager) = spawn_recording_router().await;
    let (creator, _creator_session) = account_with_session(&db, "sub-anon-creator", "Creator");
    let room_id = "aaaabbbbccccdddd";
    let room_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Anon room", creator, 1000).unwrap();

    let bridge = manager
        .get_or_create_room(room_u64, creator)
        .await
        .expect("the room starts");

    // A closed room refuses account 0 (the anonymous account).
    assert_eq!(
        bridge.query_account_permission(0).await.unwrap(),
        RoomPermissionLevel::Denied
    );

    // Opening the default permission makes anonymous a viewer.
    bridge
        .set_default_permission(creator, RoomPermissionLevel::Viewer)
        .await
        .unwrap();
    assert_eq!(
        bridge.query_account_permission(0).await.unwrap(),
        RoomPermissionLevel::Viewer
    );

    // An anonymous WebSocket is accepted once the default is not `denied`.
    let (mut anonymous, _) = connect_async(format!("ws://{addr}/ws/room/{room_id}"))
        .await
        .expect("anonymous viewer is accepted for a room whose default is viewer");
    let _ = anonymous.close(None).await;

    // Tightening it back closes the room to anonymous callers again.
    bridge
        .set_default_permission(creator, RoomPermissionLevel::Denied)
        .await
        .unwrap();
    expect_ws_rejection(
        format!("ws://{addr}/ws/room/{room_id}"),
        axum::http::StatusCode::FORBIDDEN,
    )
    .await;
}

// spec: alhalte-maitre-d/permissions 0020 unknown room is 404 without spawning
#[tokio::test]
async fn test_unknown_room_is_not_registered_or_spawned() {
    let (addr, _temp_dir, db, spawner, _manager) = spawn_recording_router().await;
    let (account_id, session_id) = account_with_session(&db, "sub-unknown-room", "Unknown");
    let unknown_room = "ffffffffffffffff";

    match connect_async(format!(
        "ws://{addr}/ws/room/{unknown_room}?account_id={account_id}&session_id={session_id}"
    ))
    .await
    {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
            assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
        }
        other => panic!("expected HTTP 404 for an unregistered room, got {other:?}"),
    }

    let export = reqwest::get(format!(
        "http://{addr}/api/room/{unknown_room}/export/zip?account_id={account_id}&session_id={session_id}"
    ))
    .await
    .unwrap();
    assert_eq!(export.status(), reqwest::StatusCode::NOT_FOUND);

    // The failed requests neither registered the room nor spawned banquette.
    assert!(db.get_room(unknown_room).unwrap().is_none());
    assert!(spawner.calls().is_empty());
}

// spec: alhalte-maitre-d/permissions 0030 export rejects a denied caller
#[tokio::test]
async fn test_export_denied_caller_is_rejected() {
    let (addr, _temp_dir, db, spawner, _manager) = spawn_recording_router().await;
    let (creator, _creator_session) = account_with_session(&db, "sub-export-creator", "Creator");
    let (outsider, outsider_session) = account_with_session(&db, "sub-export-outsider", "Outsider");
    let room_id = "fedcba9876543210";
    let room_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Private export", creator, 1000)
        .unwrap();

    let response = reqwest::get(format!(
        "http://{addr}/api/room/{room_id}/export/zip?account_id={outsider}&session_id={outsider_session}"
    ))
    .await
    .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    // The permission question is asked of the room process, which is why the
    // room is spawned even for a caller that ends up denied.
    assert_eq!(spawner.calls(), vec![(room_u64, creator, None)]);
}

// spec: alhalte-maitre-d/permissions 0031 export drops scenes the caller cannot access
#[tokio::test]
async fn test_export_filters_scenes_by_permission() {
    let (addr, _temp_dir, db, _spawner, manager) = spawn_recording_router().await;
    let (creator, creator_session) = account_with_session(&db, "sub-export-admin", "Admin");
    let (participant, participant_session) =
        account_with_session(&db, "sub-export-participant", "Participant");
    let room_id = "1122334455667788";
    let room_id_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Filtered export", creator, 1000)
        .unwrap();

    // The creator builds two extra scenes over the real WebSocket path.
    let (mut admin_ws, _) = connect_async(format!(
        "ws://{addr}/ws/room/{room_id}?account_id={creator}&session_id={creator_session}"
    ))
    .await
    .expect("admin WebSocket");
    for scene_id in ["tavern", "secret"] {
        admin_ws
            .send(WsMessage::Text(
                serde_json::to_string(&ClientFrame::CreateScene {
                    scene_id: scene_id.to_string(),
                    inherit_juices: false,
                    name: None,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
    }
    // Wait until the scenes are announced before exporting.
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), admin_ws.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::SceneList { scenes }) =
                serde_json::from_str::<ServerFrame>(&text)
            && scenes.iter().any(|scene| scene.id == "tavern")
            && scenes.iter().any(|scene| scene.id == "secret")
        {
            break;
        }
    }

    // A participant only gets `/` unless an administrator grants more, so its
    // export must not contain the admin-only scenes.
    let bridge = manager
        .get_or_create_room(room_id_u64, creator)
        .await
        .expect("the room process is reusable");
    bridge
        .set_member_permission(creator, participant, RoomPermissionLevel::Participant)
        .await
        .unwrap();

    let response = reqwest::get(format!(
        "http://{addr}/api/room/{room_id}/export/zip?type=full&account_id={participant}&session_id={participant_session}"
    ))
    .await
    .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.bytes().await.unwrap();
    let archive =
        alhalte_kitchen::RoomArchive::load_from_slice(&body, None).expect("a readable archive");

    assert!(
        archive.scenes.contains_key("/"),
        "the root scene is always exported"
    );
    assert!(
        !archive.scenes.contains_key("tavern"),
        "a participant cannot read scenes it was not granted"
    );
    assert!(
        !archive.scenes.contains_key("secret"),
        "a participant cannot read scenes it was not granted"
    );

    // The administrator's own export still contains every scene.
    let admin_export = reqwest::get(format!(
        "http://{addr}/api/room/{room_id}/export/zip?type=full&account_id={creator}&session_id={creator_session}"
    ))
    .await
    .unwrap();
    assert_eq!(admin_export.status(), reqwest::StatusCode::OK);
    let admin_body = admin_export.bytes().await.unwrap();
    let admin_archive = alhalte_kitchen::RoomArchive::load_from_slice(&admin_body, None)
        .expect("a readable archive");
    assert!(admin_archive.scenes.contains_key("tavern"));
    assert!(admin_archive.scenes.contains_key("secret"));

    let _ = admin_ws.close(None).await;
}

// spec: alhalte-maitre-d/permissions 0032 the bridge admin is the room creator
#[tokio::test]
async fn test_bridge_admin_is_creator_not_first_connector() {
    let (addr, _temp_dir, db, spawner, manager) = spawn_recording_router().await;
    let (creator, _creator_session) = account_with_session(&db, "sub-admin-creator", "Creator");
    let (player, player_session) = account_with_session(&db, "sub-admin-player", "Player");
    let room_id = "abcdefabcdef0001";
    let room_u64 = u64::from_str_radix(room_id, 16).unwrap();
    db.create_room(room_id, "Admin room", creator, 1000)
        .unwrap();

    // The player is not a member, so they are denied -- but answering that
    // question spawns the room. The spawn must still use the creator, not the
    // first caller, because `Room::new` makes its `admin_account` the Admin.
    expect_ws_rejection(
        format!("ws://{addr}/ws/room/{room_id}?account_id={player}&session_id={player_session}"),
        axum::http::StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(spawner.calls(), vec![(room_u64, creator, None)]);

    let bridge = manager
        .get_or_create_room(room_u64, creator)
        .await
        .expect("the room process is live");
    assert_eq!(
        bridge.query_account_permission(creator).await.unwrap(),
        RoomPermissionLevel::Admin,
        "the creator is the room Admin even though the player connected first"
    );
    assert_eq!(
        bridge.query_account_permission(player).await.unwrap(),
        RoomPermissionLevel::Denied
    );
}

// spec: alhalte-maitre-d/cli-args chunk-size-exponent
#[test]
fn test_chunk_size_exponent_cli_flag() {
    let default = parse_cli_args_from(["alhalte-maitre-d"]).unwrap();
    assert_eq!(default.chunk_size_exponent, 8);

    let custom = parse_cli_args_from(["alhalte-maitre-d", "--chunk-size-exponent", "12"]).unwrap();
    assert_eq!(custom.chunk_size_exponent, 12);
}

// spec: alhalte-maitre-d/cli-args internal cellar service token
#[test]
fn test_service_token_cli_flag() {
    let default = parse_cli_args_from(["alhalte-maitre-d"]).unwrap();
    assert_eq!(default.service_token, None);

    let with_token =
        parse_cli_args_from(["alhalte-maitre-d", "--service-token", "internal-secret"]).unwrap();
    assert_eq!(with_token.service_token.as_deref(), Some("internal-secret"));
}

// spec: alhalte-maitre-d/websocket-permission-gate 0001 denied is 403 before the upgrade
#[tokio::test]
async fn test_websocket_denied_caller_rejected_and_default_viewer_permitted() {
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
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager.clone());
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 901;

    // The creator is banquette's initial Admin; nothing is written to SQLite.
    register_room(&db, room_id, gm.account_id, "Private room");

    // 1. A non-member is denied: banquette is asked, and the answer is 403.
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

    // 2. An anonymous caller is denied for a room whose default is `denied`.
    let anon_ws_url = format!("ws://{addr}/ws/room/{room_id}");
    match connect_async(&anon_ws_url).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => {
            assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
        }
        other => panic!("Expected 403 Forbidden rejection, got: {other:?}"),
    }

    // 3. The Admin connects (the creator is the room Admin).
    let gm_ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        gm.account_id, gm_sess.session_id
    );
    let (mut gm_stream, _) = connect_async(&gm_ws_url).await.unwrap();

    // 4. The Admin opens the room to viewers through banquette's default
    //    permission (the four-level model replaced the `done` flag, Issue #199).
    let set_default_permission = serde_json::to_string(&ClientFrame::SetDefaultPermission {
        permission: RoomPermissionLevel::Viewer,
    })
    .unwrap();
    gm_stream
        .send(WsMessage::Text(set_default_permission.into()))
        .await
        .unwrap();

    let mut acknowledged = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(t))) =
            tokio::time::timeout(Duration::from_millis(500), gm_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::PermissionsUpdated { .. }) =
                serde_json::from_str::<ServerFrame>(&t)
        {
            acknowledged = true;
            break;
        }
    }
    assert!(
        acknowledged,
        "the room must acknowledge the permission change"
    );

    // 5. The outsider now passes the gate as a viewer.
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
            assert!(scenes.iter().any(|s| s.id == "/"));
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
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 902;
    register_room(&db, room_id, user.account_id, "Cookie room");

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
            assert!(scenes.iter().any(|s| s.id == "/"));
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

    // Register the room (the creator becomes banquette's initial Admin), start
    // the room process, and grant the viewer through the room permission table.
    register_room(&db, room_id, gm.account_id, "Viewer room");
    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let bridge = room_manager
        .get_or_create_room(room_id, gm.account_id)
        .await
        .expect("the room process starts");
    bridge
        .set_member_permission(
            gm.account_id,
            viewer.account_id,
            RoomPermissionLevel::Viewer,
        )
        .await
        .expect("the creator can grant viewer");
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
            assert_eq!(code, "AppendLogFailed");
            assert!(message.to_lowercase().contains("permission denied"));
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
            && let Ok(ServerFrame::Error { code, message }) =
                serde_json::from_str::<ServerFrame>(&t)
        {
            assert!(code.ends_with("Failed"));
            assert!(message.to_lowercase().contains("permission denied"));
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
            && let Ok(ServerFrame::Error { code, message }) =
                serde_json::from_str::<ServerFrame>(&t)
        {
            assert!(code.ends_with("Failed"));
            assert!(message.to_lowercase().contains("permission denied"));
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

// spec: alhalte-maitre-d/gateway initial scene demux
#[tokio::test]
async fn test_initial_scene_demultiplexing_and_root_broadcasts() {
    let temp_dir = tempdir().unwrap();
    let socket_dir = temp_dir.path().to_path_buf();

    let db = Database::new_in_memory().unwrap();
    let acc = db
        .get_or_create_account("sub-init-scene", "DemuxUser", None, 1000)
        .unwrap();
    let session = db.create_session(acc.account_id, 1000).unwrap();

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        socket_dir, 8,
    ));
    let room_manager = RoomBridgeManager::new(spawner, db.clone());
    let oauth = Arc::new(MockOAuthProvider::new());
    let state = AppState::new(db.clone(), "salt", oauth, room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let room_id: u64 = 999;
    register_room(&db, room_id, acc.account_id, "Initial scene room");
    let ws_url = format!(
        "ws://{addr}/ws/room/{room_id}?account_id={}&session_id={}",
        acc.account_id, session.session_id
    );

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();

    // The client is connected and registered into root scene "/" by default.
    // Send an update to root scene state
    let state_tin = tin::Tin::from_string("Initial Root Scene State");
    let update_frame = serde_json::to_string(&ClientFrame::UpdateState {
        scene_id: "/".to_string(),
        key: "system::welcome".to_string(),
        value: state_tin.clone(),
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(update_frame.into()))
        .await
        .unwrap();

    // Verify client receives StateUpdated for "/" without needing explicit EnterScene
    let mut received_root_state = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::StateUpdated {
                scene_id,
                key,
                value,
            }) = serde_json::from_str::<ServerFrame>(&text)
            && scene_id == "/"
            && key == "system::welcome"
            && value == state_tin
        {
            received_root_state = true;
            break;
        }
    }
    assert!(
        received_root_state,
        "Client should receive root scene StateUpdated broadcast upon initial connection"
    );

    // Send log entry to root scene "/"
    let log_entry = tin::Tin::from_string("Welcome to the room!");
    let log_frame = serde_json::to_string(&ClientFrame::AppendLog {
        scene_id: "/".to_string(),
        entry: Some(log_entry.clone()),
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(log_frame.into()))
        .await
        .unwrap();

    // Verify client receives LogEventAppended for "/"
    let mut received_root_log = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::LogEventAppended {
                scene_id, entry, ..
            }) = serde_json::from_str::<ServerFrame>(&text)
            && scene_id == "/"
            && entry == Some(log_entry.clone())
        {
            received_root_log = true;
            break;
        }
    }
    assert!(
        received_root_log,
        "Client should receive root scene LogEventAppended broadcast upon initial connection"
    );

    // Create and move to another scene "side-room"
    let create_scene = serde_json::to_string(&ClientFrame::CreateScene {
        scene_id: "side-room".to_string(),
        inherit_juices: false,
        name: None,
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(create_scene.into()))
        .await
        .unwrap();

    let enter_side = serde_json::to_string(&ClientFrame::EnterScene {
        scene_id: "side-room".to_string(),
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(enter_side.into()))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Update "side-room" state
    let side_state = tin::Tin::from_string("Side Room Ambient");
    let side_update = serde_json::to_string(&ClientFrame::UpdateState {
        scene_id: "side-room".to_string(),
        key: "side::ambient".to_string(),
        value: side_state.clone(),
    })
    .unwrap();
    ws_stream
        .send(WsMessage::Text(side_update.into()))
        .await
        .unwrap();

    // Verify client receives StateUpdated for "side-room"
    let mut received_side_state = false;
    for _ in 0..20 {
        if let Some(Ok(WsMessage::Text(text))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next())
                .await
                .ok()
                .flatten()
            && let Ok(ServerFrame::StateUpdated {
                scene_id,
                key,
                value,
            }) = serde_json::from_str::<ServerFrame>(&text)
            && scene_id == "side-room"
            && key == "side::ambient"
            && value == side_state
        {
            received_side_state = true;
            break;
        }
    }
    assert!(
        received_side_state,
        "Client should receive StateUpdated broadcast for new scene 'side-room'"
    );

    ws_stream.close(None).await.unwrap();
}

// spec: alhalte-maitre-d/spawner binary path resolution
#[test]
fn test_resolve_banquette_bin_paths() {
    use std::path::Path;

    // Existing file returns immediately
    if let Ok(exe) = std::env::current_exe() {
        let resolved = resolve_banquette_bin(&exe);
        assert_eq!(resolved, exe);
    }

    // Resolving standard name "alhalte-banquette"
    let bin_name = Path::new("alhalte-banquette");
    let resolved = resolve_banquette_bin(bin_name);
    // If built in target/debug, it should resolve to target/debug/alhalte-banquette
    if Path::new("target/debug/alhalte-banquette").is_file() {
        assert_eq!(
            resolved,
            std::path::PathBuf::from("target/debug/alhalte-banquette")
        );
    }

    // Non-existent binary falls back to original path
    let dummy = Path::new("nonexistent-banquette-bin-xyz-9876");
    let fallback = resolve_banquette_bin(dummy);
    assert_eq!(fallback, dummy);
}
