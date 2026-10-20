//! Debug login tests (Issue #150).
//!
//! These exercise the isolated debug login end to end on a loopback listener and
//! the real manager / in-process banquette stack. No external network is used.

use std::path::Path;
use std::sync::Arc;

use alhalte_maitre_d::{
    AppState, CliArgs, DEBUG_SUBJECT_PREFIX, Database, InProcessBanquetteSpawner,
    MAX_DEBUG_ACCOUNT_NAME_LEN, MockOAuthProvider, OAuthProvider, ProcessBanquetteSpawner,
    RoomBridgeManager, ServerInfoResponse, SessionResponse, SubHash, create_router, hash_sub,
    parse_cli_args_from,
};
use serde_json::json;
use tempfile::tempdir;

/// Browser origin allowed by the test router.
const APP_ORIGIN: &str = "http://app.test";

/// Instance salt used by the test router (must match `debug_state`).
const TEST_SALT: &str = "debug-salt";

/// Hashes a test `sub` the way the provider path does (Issue #190).
fn test_sub_hash(sub: &str) -> SubHash {
    let mut sub = sub.to_string();
    hash_sub("test-salt", &mut sub)
}

/// Builds the debug-login state used by the HTTP tests.
fn debug_state(db: Database) -> AppState {
    debug_state_with_lan_ports(db, &[])
}

/// Builds the debug-login state with the optional Issue #229 LAN switch.
fn debug_state_with_lan_ports(db: Database, lan_origin_ports: &[u16]) -> AppState {
    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        "/tmp/alhalte-debug-login-tests",
        8,
    ));
    let manager = RoomBridgeManager::new(spawner);
    AppState::new(db, TEST_SALT, Arc::new(MockOAuthProvider::new()), manager)
        .with_public_url("http://maitre-d.test")
        .with_allowed_origins([APP_ORIGIN])
        .with_lan_origin_ports(lan_origin_ports.iter().copied())
        .with_debug_login()
}

/// Builds the normal Google state (no debug login).
fn google_state(db: Database, oauth: Arc<dyn OAuthProvider>) -> AppState {
    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        "/tmp/alhalte-debug-login-tests",
        8,
    ));
    let manager = RoomBridgeManager::new(spawner);
    AppState::new(db, TEST_SALT, oauth, manager)
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

/// Login is a browser navigation flow: never follow redirects in tests.
fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// Extracts the `session_id` from the first `Set-Cookie` header of `response`.
fn session_cookie(response: &reqwest::Response) -> String {
    response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .find_map(|value| value.to_str().ok())
        .expect("the debug login must set the session cookie")
        .to_string()
}

/// Splits the `session_id` value out of a `Set-Cookie` header.
fn session_id_from_cookie(cookie: &str) -> u64 {
    cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("session_id=")
        .parse()
        .expect("session_id is a decimal u64")
}

/// Submits the debug login form.
async fn submit_debug_login(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    return_to: &str,
    account_name: &str,
) -> reqwest::Response {
    client
        .post(format!("http://{addr}/auth/debug/login"))
        .form(&[("return_to", return_to), ("account_name", account_name)])
        .send()
        .await
        .unwrap()
}

// spec: alhalte-maitre-d/debug-login 0005 GET /api/server-info reports the flag
#[tokio::test]
async fn server_info_reports_whether_debug_login_is_active() {
    let addr = serve(create_router(debug_state(
        Database::new_in_memory().unwrap(),
    )))
    .await;
    let info: ServerInfoResponse = reqwest::get(format!("http://{addr}/api/server-info"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(info.debug_login);

    let addr = serve(create_router(google_state(
        Database::new_in_memory().unwrap(),
        Arc::new(MockOAuthProvider::new()),
    )))
    .await;
    let info: ServerInfoResponse = reqwest::get(format!("http://{addr}/api/server-info"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!info.debug_login);
}

// spec: alhalte-maitre-d/debug-login 0006 /auth/login hands the browser to the form
#[tokio::test]
async fn debug_login_redirects_auth_login_to_the_debug_form() {
    let addr = serve(create_router(debug_state(
        Database::new_in_memory().unwrap(),
    )))
    .await;
    let client = no_redirect_client();

    let response = client
        .get(format!("http://{addr}/auth/login"))
        .query(&[("return_to", "http://app.test/rooms/abc?invite=1")])
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
        "/auth/debug/login?return_to=http%3A%2F%2Fapp.test%2Frooms%2Fabc%3Finvite%3D1",
        "the debug form is a local redirect and return_to keeps its encoding"
    );

    // The open-redirect guard still runs before the provider is asked.
    for bad_return_to in [
        "http://evil.example/",
        "//evil.example",
        "javascript:alert(1)",
        "",
    ] {
        let response = client
            .get(format!("http://{addr}/auth/login"))
            .query(&[("return_to", bad_return_to)])
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "return_to {bad_return_to:?} must be rejected before the debug redirect"
        );
    }
}

// spec: alhalte-maitre-d/lan-origins 0002 a LAN phone completes the debug login
// return_to only while the explicit dev switch is on (Issue #229)
#[tokio::test]
async fn debug_login_accepts_a_lan_return_to_only_with_the_dev_switch() {
    let lan_return_to = "http://192.168.1.42:5173/rooms/abc";
    let client = no_redirect_client();

    // Switch off: the LAN origin is just another foreign origin.
    let addr = serve(create_router(debug_state(
        Database::new_in_memory().unwrap(),
    )))
    .await;
    let blocked = client
        .get(format!("http://{addr}/auth/debug/login"))
        .query(&[("return_to", lan_return_to)])
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), reqwest::StatusCode::BAD_REQUEST);

    // Switch on: the form is served, the login succeeds, and the session cookie
    // is set before redirecting back to the LAN origin. The Vite /auth proxy
    // makes the response same-origin for the phone.
    let addr = serve(create_router(debug_state_with_lan_ports(
        Database::new_in_memory().unwrap(),
        &[5173],
    )))
    .await;
    let form = client
        .get(format!("http://{addr}/auth/debug/login"))
        .query(&[("return_to", lan_return_to)])
        .send()
        .await
        .unwrap();
    assert_eq!(form.status(), reqwest::StatusCode::OK);
    let body = form.text().await.unwrap();
    assert!(
        body.contains(&format!("value=\"{lan_return_to}\"")),
        "the form must keep the LAN return_to in its hidden field"
    );

    let response = submit_debug_login(&client, addr, lan_return_to, "Alice").await;
    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        lan_return_to
    );
    assert!(session_cookie(&response).starts_with("session_id="));
}

// spec: alhalte-maitre-d/debug-login 0007 the form only asks for an account name
#[tokio::test]
async fn debug_login_form_renders_an_account_name_input() {
    let addr = serve(create_router(debug_state(
        Database::new_in_memory().unwrap(),
    )))
    .await;

    let response = reqwest::get(format!(
        "http://{addr}/auth/debug/login?return_to=http%3A%2F%2Fapp.test%2F"
    ))
    .await
    .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap(),
        "no-store"
    );

    let body = response.text().await.unwrap();
    assert!(body.contains("name=\"account_name\""));
    assert!(body.contains("action=\"/auth/debug/login\""));
    assert!(body.contains("method=\"post\""));
    assert!(
        body.contains("value=\"http://app.test/\""),
        "return_to must survive as a hidden field"
    );
    assert!(body.contains(&format!("maxlength=\"{MAX_DEBUG_ACCOUNT_NAME_LEN}\"")));

    // A missing or foreign return_to is rejected.
    let missing = reqwest::get(format!("http://{addr}/auth/debug/login"))
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::BAD_REQUEST);
    let foreign = reqwest::get(format!(
        "http://{addr}/auth/debug/login?return_to=http%3A%2F%2Fevil.example%2F"
    ))
    .await
    .unwrap();
    assert_eq!(foreign.status(), reqwest::StatusCode::BAD_REQUEST);
}

// spec: alhalte-maitre-d/debug-login 0008 a name alone creates / reuses the account
#[tokio::test]
async fn debug_login_creates_or_reuses_an_account_from_the_account_name() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(debug_state(db.clone()))).await;
    let client = no_redirect_client();

    let response = submit_debug_login(
        &client,
        addr,
        "http://app.test/rooms/abc?invite=1",
        "  Alice  ",
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "http://app.test/rooms/abc?invite=1"
    );
    let cookie = session_cookie(&response);
    assert!(cookie.contains("SameSite=Lax"));
    assert!(
        !cookie.contains("Secure"),
        "an http public URL has no Secure cookie"
    );
    let session_id = session_id_from_cookie(&cookie);

    // The cookie resolves to the account named after the input.
    let session: SessionResponse = client
        .get(format!("http://{addr}/api/session"))
        .header(reqwest::header::COOKIE, format!("session_id={session_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.account.handle_name, "Alice");

    // The stored key is the salted hash of the name-derived subject: the name is
    // a credential, not a plain-text column.
    let stored = db.get_account(session.account.account_id).unwrap().unwrap();
    let mut subject = format!("{DEBUG_SUBJECT_PREFIX}Alice");
    assert_eq!(stored.sub_hash, hash_sub(TEST_SALT, &mut subject));

    // The same name reuses the account and issues a fresh session.
    let again = submit_debug_login(&client, addr, "http://app.test/", "Alice").await;
    assert_eq!(again.status(), reqwest::StatusCode::FOUND);
    let again_cookie = session_cookie(&again);
    let again_session_id = session_id_from_cookie(&again_cookie);
    assert_ne!(again_session_id, session_id);
    let again_session: SessionResponse = client
        .get(format!("http://{addr}/api/session"))
        .header(
            reqwest::header::COOKIE,
            format!("session_id={again_session_id}"),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        again_session.account.account_id, session.account.account_id,
        "the same name maps back to the same account"
    );

    // A different name creates a different account.
    let bob = submit_debug_login(&client, addr, "http://app.test/", "Bob").await;
    assert_eq!(bob.status(), reqwest::StatusCode::FOUND);
    let bob_session_id = session_id_from_cookie(&session_cookie(&bob));
    let bob_session: SessionResponse = client
        .get(format!("http://{addr}/api/session"))
        .header(
            reqwest::header::COOKIE,
            format!("session_id={bob_session_id}"),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(bob_session.account.account_id, session.account.account_id);
    assert_eq!(bob_session.account.handle_name, "Bob");
}

// spec: alhalte-maitre-d/debug-login 0009 unusable account names are rejected
#[tokio::test]
async fn debug_login_rejects_unusable_account_names() {
    let db = Database::new_in_memory().unwrap();
    let addr = serve(create_router(debug_state(db.clone()))).await;
    let client = no_redirect_client();

    let long_name = "x".repeat(MAX_DEBUG_ACCOUNT_NAME_LEN + 1);
    for name in ["", "   ", "bad\nname", long_name.as_str()] {
        let response = submit_debug_login(&client, addr, "http://app.test/", name).await;
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "account_name {name:?} must be rejected"
        );
    }
    assert!(
        db.search_accounts("x").unwrap().is_empty(),
        "a rejected name must not create an account"
    );

    // The return_to guard applies to the POST too.
    let response = submit_debug_login(&client, addr, "http://evil.example/", "Alice").await;
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

// spec: alhalte-maitre-d/debug-login 0010 the flows do not coexist
#[tokio::test]
async fn debug_login_and_google_routes_are_mutually_exclusive() {
    let client = no_redirect_client();

    // Debug login: the Google routes do not exist.
    let addr = serve(create_router(debug_state(
        Database::new_in_memory().unwrap(),
    )))
    .await;
    let callback = client
        .get(format!("http://{addr}/auth/google/callback?code=x&state=y"))
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), reqwest::StatusCode::NOT_FOUND);
    let token = client
        .post(format!("http://{addr}/auth/google/token"))
        .json(&json!({ "id_token": "id-token" }))
        .send()
        .await
        .unwrap();
    assert_eq!(token.status(), reqwest::StatusCode::NOT_FOUND);

    // Normal mode: the debug form does not exist and Google still works.
    let mock = Arc::new(MockOAuthProvider::new());
    let addr = serve(create_router(google_state(
        Database::new_in_memory().unwrap(),
        mock,
    )))
    .await;
    let form = client
        .get(format!(
            "http://{addr}/auth/debug/login?return_to=http%3A%2F%2Fapp.test%2F"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(form.status(), reqwest::StatusCode::NOT_FOUND);
    let submit = submit_debug_login(&client, addr, "http://app.test/", "Alice").await;
    assert_eq!(submit.status(), reqwest::StatusCode::NOT_FOUND);

    let login = client
        .get(format!("http://{addr}/auth/login"))
        .query(&[("return_to", "http://app.test/")])
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), reqwest::StatusCode::FOUND);
    let location = login
        .headers()
        .get(reqwest::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        location.starts_with("https://mock-google-oauth.example.com/auth?"),
        "the Google login path is unchanged when the debug login is off: {location}"
    );
}

/// Row counts of the tables a debug login must never grow.
#[derive(Debug, PartialEq, Eq)]
struct TableCounts {
    accounts: i64,
    sessions: i64,
    rooms: i64,
}

/// Counts `accounts` / `sessions` / `rooms` rows in the SQLite file at `path`.
fn table_counts(path: &Path) -> TableCounts {
    let conn = rusqlite::Connection::open(path).unwrap();
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    };
    TableCounts {
        accounts: count("accounts"),
        sessions: count("sessions"),
        rooms: count("rooms"),
    }
}

// spec: alhalte-maitre-d/debug-login 0001 debug login never touches the normal DB
#[tokio::test]
async fn debug_login_uses_a_separate_database_file() {
    let dir = tempdir().unwrap();
    let normal_path = dir.path().join("maitre-d.sqlite3");
    let expected_debug_path = dir.path().join("maitre-d-debug.sqlite3");

    // Seed the normal database with an account, a session, and a room.
    let normal_db = Database::open(&normal_path).unwrap();
    let normal_account = normal_db
        .get_or_create_account(&test_sub_hash("sub-normal"), "Normal", None, 1000)
        .unwrap();
    normal_db
        .create_session(normal_account.account_id, 1000)
        .unwrap();
    normal_db
        .create_room(
            "00000000000000aa",
            "Normal room",
            normal_account.account_id,
            1000,
        )
        .unwrap();
    let before = table_counts(&normal_path);
    assert_eq!(before.accounts, 1);
    assert_eq!(before.sessions, 1);
    assert_eq!(before.rooms, 1);

    // `--debug-login` selects the sibling debug file, never the normal one.
    let args: CliArgs = parse_cli_args_from([
        "alhalte-maitre-d",
        "--db-path",
        normal_path.to_str().unwrap(),
        "--debug-login",
    ])
    .unwrap();
    assert!(args.debug_login);
    assert_eq!(args.active_db_path(), expected_debug_path);

    // A debug login against that file creates the account and session there.
    let debug_db = Database::open(args.active_db_path()).unwrap();
    let addr = serve(create_router(debug_state(debug_db.clone()))).await;
    let client = no_redirect_client();
    let response = submit_debug_login(&client, addr, "http://app.test/", "Alice").await;
    assert_eq!(response.status(), reqwest::StatusCode::FOUND);

    // The normal database is byte-for-byte untouched at the row level.
    assert_eq!(table_counts(&normal_path), before);
    assert!(
        normal_db.search_accounts("Alice").unwrap().is_empty(),
        "the debug account must not exist in the normal database"
    );

    // The debug file holds exactly the debug account and its session.
    let debug_counts = table_counts(&expected_debug_path);
    assert_eq!(debug_counts.accounts, 1);
    assert_eq!(debug_counts.sessions, 1);
    assert_eq!(debug_counts.rooms, 0);
}

// spec: alhalte-maitre-d/debug-login 0002 debug banquette runs without persistence
#[test]
fn process_spawner_disables_persistence_for_debug_rooms() {
    let socket = Path::new("/run/alhalte/banquette-1.sock");

    let normal = ProcessBanquetteSpawner::new(
        "/usr/local/bin/alhalte-banquette",
        "/run/alhalte",
        "http://127.0.0.1:8081",
        8,
    );
    assert!(!normal.is_no_persist());
    assert!(
        !normal
            .command_args(1, 42, socket)
            .iter()
            .any(|arg| arg == "--no-persist")
    );

    let debug = ProcessBanquetteSpawner::new(
        "/usr/local/bin/alhalte-banquette",
        "/run/alhalte",
        "http://127.0.0.1:8081",
        8,
    )
    .with_no_persist();
    assert!(debug.is_no_persist());
    let args = debug.command_args(1, 42, socket);
    assert!(
        args.iter().any(|arg| arg == "--no-persist"),
        "the child must keep room state in memory: {args:?}"
    );
}
