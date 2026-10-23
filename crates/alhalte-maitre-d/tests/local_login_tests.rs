//! Local password login tests.
//!
//! These exercise the JSON login API, the separate local database, the SPA
//! redirect, and the additive `accounts.password_hash` migration. No external
//! network is used.

use std::sync::Arc;

use alhalte_maitre_d::{
    AppState, BanquetteEndpoint, BanquetteSpawner, Database, LocalAccount, MaitreDError,
    MockOAuthProvider, RoomBridgeManager, SubHash, create_router, hash_password, hash_sub,
    parse_cli_args_from, verify_password,
};
use serde_json::json;
use tempfile::tempdir;

/// Browser origin allowed by the test router.
const APP_ORIGIN: &str = "http://app.test";

/// Instance salt used by the test router and the local subject hashes.
const TEST_SALT: &str = "local-login-test-salt";

/// Spawner that must never be called by the login tests.
struct FailingSpawner;

#[async_trait::async_trait]
impl BanquetteSpawner for FailingSpawner {
    async fn spawn_banquette(
        &self,
        _room_id: u64,
        _admin_account: u64,
    ) -> alhalte_maitre_d::Result<BanquetteEndpoint> {
        Err(MaitreDError::Banquette(
            "room spawn is not used by local login tests".to_string(),
        ))
    }
}

/// Builds a local-login state backed by `db`.
fn local_state(db: Database) -> AppState {
    let manager = RoomBridgeManager::new(Arc::new(FailingSpawner));
    AppState::new(db, TEST_SALT, Arc::new(MockOAuthProvider::new()), manager)
        .with_public_url("http://maitre-d.test")
        .with_allowed_origins([APP_ORIGIN])
        .with_local_login()
}

/// Builds a Google state backed by `db`.
fn google_state(db: Database) -> AppState {
    let manager = RoomBridgeManager::new(Arc::new(FailingSpawner));
    AppState::new(db, TEST_SALT, Arc::new(MockOAuthProvider::new()), manager)
        .with_public_url("http://maitre-d.test")
        .with_allowed_origins([APP_ORIGIN])
}

/// Serves `app` on a loopback port and returns its address.
async fn serve(app: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// Login is a JSON API; redirects are tested explicitly, never followed.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// Extracts the `session_id=...` Cookie header value.
fn session_cookie(response: &reqwest::Response) -> String {
    let header = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("local login must set a session cookie")
        .to_str()
        .unwrap();
    header.split(';').next().expect("cookie value").to_string()
}

/// Hashes a local account name exactly like the provider does.
fn local_sub_hash(name: &str) -> SubHash {
    let mut subject = format!("alhalte/local-login/{name}");
    hash_sub(TEST_SALT, &mut subject)
}

// spec: alhalte-maitre-d/local-login 0007 the local DB is selected independently
#[test]
fn local_login_selects_the_local_database_file() {
    let normal = parse_cli_args_from(["alhalte-maitre-d"]).unwrap();
    assert_eq!(
        normal.active_db_path().to_string_lossy(),
        "maitre-d.sqlite3"
    );

    let local = parse_cli_args_from(["alhalte-maitre-d", "--local-login"]).unwrap();
    assert_eq!(
        local.active_db_path().to_string_lossy(),
        "maitre-d-local.sqlite3"
    );
    assert_ne!(local.active_db_path(), local.db_path);
}

// spec: alhalte-maitre-d/local-login 0008 a new name creates a password account
#[tokio::test]
async fn local_login_creates_an_account_and_issues_a_session() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(local_state(db.clone()))).await;
    let client = client();

    let response = client
        .post(format!("http://{addr}/api/auth/local/login"))
        .json(&json!({ "account_name": "  Alice  ", "password": "correct horse" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let cookie = session_cookie(&response);

    // The account is keyed by the salted local subject, not by the raw name.
    let local = db
        .get_local_account(&local_sub_hash("Alice"))
        .unwrap()
        .expect("local account must exist");
    assert_eq!(local.account.handle_name, "Alice");
    let stored = local.password_hash.expect("password hash must be stored");
    assert!(verify_password("correct horse", &stored).unwrap());
    assert!(!stored.contains("correct horse"));

    // The session cookie resolves through the normal /api/session path.
    let session = client
        .get(format!("http://{addr}/api/session"))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(session.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = session.json().await.unwrap();
    assert_eq!(body["account"]["handle_name"], "Alice");
}

// spec: alhalte-maitre-d/local-login 0009 an existing name verifies the password
#[tokio::test]
async fn local_login_reuses_an_account_and_rejects_a_wrong_password() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(local_state(db.clone()))).await;
    let client = client();

    let first = client
        .post(format!("http://{addr}/api/auth/local/login"))
        .json(&json!({ "account_name": "Bob", "password": "first-password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), reqwest::StatusCode::OK);

    let again = client
        .post(format!("http://{addr}/api/auth/local/login"))
        .json(&json!({ "account_name": "Bob", "password": "first-password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), reqwest::StatusCode::OK);

    let wrong = client
        .post(format!("http://{addr}/api/auth/local/login"))
        .json(&json!({ "account_name": "Bob", "password": "wrong-password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        db.search_accounts("Bob").unwrap().len(),
        1,
        "a wrong password must not create another account"
    );
}

// spec: alhalte-maitre-d/local-login 0010 invalid input is rejected before hashing
#[tokio::test]
async fn local_login_rejects_short_passwords_and_bad_names() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(local_state(db))).await;
    let client = client();

    for (name, password) in [
        ("Alice", "short"),
        ("   ", "long-enough-password"),
        ("bad\nname", "long-enough-password"),
    ] {
        let response = client
            .post(format!("http://{addr}/api/auth/local/login"))
            .json(&json!({ "account_name": name, "password": password }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "unexpected status for name={name:?}"
        );
    }
}

// spec: alhalte-maitre-d/local-login 0011 /auth/login hands the browser to the SPA
#[tokio::test]
async fn local_login_redirects_auth_login_to_the_spa() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(local_state(db))).await;
    let response = client()
        .get(format!(
            "http://{addr}/auth/login?return_to=http%3A%2F%2Fapp.test%2Frooms%2F1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "/login?return_to=http%3A%2F%2Fapp.test%2Frooms%2F1"
    );
}

// spec: alhalte-maitre-d/local-login 0012 the local API does not exist in Google mode
#[tokio::test]
async fn local_login_api_is_404_in_google_mode() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(google_state(db))).await;
    let response = client()
        .post(format!("http://{addr}/api/auth/local/login"))
        .json(&json!({ "account_name": "Alice", "password": "long-enough-password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

// spec: alhalte-maitre-d/local-login 0013 password_hash migrates additively
#[test]
fn password_hash_column_is_added_to_an_existing_database() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("old.sqlite3");
    let sub_hash = local_sub_hash("Alice");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE accounts (
                account_id INTEGER PRIMARY KEY,
                sub_hash TEXT NOT NULL UNIQUE,
                handle_name TEXT NOT NULL,
                icon_tin_id BLOB,
                created_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (account_id, sub_hash, handle_name, icon_tin_id, created_at)
             VALUES (1, ?1, 'Alice', NULL, 1000)",
            rusqlite::params![sub_hash.as_hex()],
        )
        .unwrap();
    }

    let db = Database::open(&path).unwrap();
    let local: LocalAccount = db
        .get_local_account(&sub_hash)
        .unwrap()
        .expect("the old account must remain readable");
    assert_eq!(local.account.account_id, 1);
    assert_eq!(local.password_hash, None);

    let hash = hash_password("migrated-password").unwrap();
    assert!(db.set_local_password_hash(1, &hash).unwrap());
    let local = db.get_local_account(&sub_hash).unwrap().unwrap();
    assert!(verify_password("migrated-password", local.password_hash.as_deref().unwrap()).unwrap());
}
