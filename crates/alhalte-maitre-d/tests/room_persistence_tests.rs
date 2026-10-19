//! Room state persistence, restore, and internal credential tests (Issue #157).
//!
//! These tests exercise the real manager / spawner / banquette server stack with
//! an in-memory or loopback cellar. No external network is used.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alhalte_banquette::{
    CellarSink, ClientFrame, HttpCellarSink, InMemoryCellarSink, Room, RoomPermissionLevel,
    ServerFrame,
};
use alhalte_cellar::{CellarService, InMemorySessionVerifier};
use alhalte_maitre_d::{
    AppState, Database, InProcessBanquetteSpawner, MockOAuthProvider, ProcessBanquetteSpawner,
    RoomBridgeManager, create_router,
};
use tempfile::tempdir;
use tin::Tin;

/// Room ID used by the persistence tests (canonical 16-hex form).
const ROOM_ID_HEX: &str = "9f3a7c1e5b2d8046";
const ROOM_ID: u64 = 0x9f3a7c1e5b2d8046;
const ADMIN: u64 = 1;

/// Spawns a loopback cellar with the given optional internal service token.
async fn spawn_cellar(
    service_token: Option<&str>,
) -> (std::net::SocketAddr, Arc<CellarService>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    let mut service = CellarService::new_with_in_memory_db(dir.path(), verifier, false).unwrap();
    if let Some(token) = service_token {
        service = service.with_service_token(token);
    }
    let service = Arc::new(service);
    let app = Arc::clone(&service).into_router();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, service, dir)
}

/// Creates a registered room whose creator becomes banquette's initial Admin.
fn registered_room(db: &Database) -> (u64, u64) {
    let account = db
        .get_or_create_account("sub-persist-admin", "Admin", None, 1000)
        .unwrap();
    db.create_room(ROOM_ID_HEX, "Persistence room", account.account_id, 1000)
        .unwrap();
    let session = db.create_session(account.account_id, 1000).unwrap();
    (account.account_id, session.session_id)
}

/// Reads frames from a client receiver until `pred` matches or the timeout expires.
async fn wait_for_frame(
    rx: &mut tokio::sync::mpsc::Receiver<ServerFrame>,
    mut pred: impl FnMut(&ServerFrame) -> bool,
) -> ServerFrame {
    for _ in 0..50 {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Some(frame)) => {
                if pred(&frame) {
                    return frame;
                }
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    panic!("expected server frame was not received");
}

/// Polls the database until the room head is recorded, returning it.
async fn wait_for_room_head(db: &Database) -> [u8; 32] {
    for _ in 0..100 {
        if let Some(head) = db.get_room_head(ROOM_ID_HEX).unwrap() {
            return head;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("maitre-d did not record a room head");
}

/// Returns the value following `flag` in an argument vector.
fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// Reads the repository's default `Bistrot.toml` and returns the parsed config.
///
/// The bundled local development configuration keeps plaintext secrets, so it is
/// parsed with the documented development escape hatch (Issue #194).
fn default_bistrot_config() -> alhalte_bistrot::BistrotConfig {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("Bistrot.toml");
    alhalte_bistrot::load_from_path_with_options(
        &path,
        &alhalte_bistrot::ParseOptions {
            age_key: None,
            allow_plaintext_secrets: true,
        },
    )
    .expect("the default Bistrot.toml must parse")
}

// spec: alhalte-maitre-d/room-head 0002 whole room survives a banquette restart
#[tokio::test]
async fn test_room_state_restored_after_banquette_restart() {
    let temp_dir = tempdir().unwrap();
    let db = Database::new_in_memory().unwrap();
    let (account_id, session_id) = registered_room(&db);

    let spawner = Arc::new(InProcessBanquetteSpawner::new_with_in_memory_cellar(
        temp_dir.path().to_path_buf(),
        8,
    ));
    let manager = RoomBridgeManager::new(spawner.clone(), db.clone());

    let bridge = manager
        .get_or_create_room(ROOM_ID, account_id)
        .await
        .expect("new room starts");
    let mut rx = bridge.register_client(1, account_id, session_id).await;

    // Populate scenes, state, characters, logs, admins, and done.
    bridge.send_client_frame(
        1,
        ClientFrame::CreateScene {
            scene_id: "tavern".to_string(),
            inherit_juices: false,
            name: None,
        },
    );
    wait_for_frame(
        &mut rx,
        |frame| matches!(frame, ServerFrame::SceneCreated { scene_id } if scene_id == "tavern"),
    )
    .await;

    bridge.send_client_frame(
        1,
        ClientFrame::UpdateState {
            scene_id: "tavern".to_string(),
            key: "custom::weather".to_string(),
            value: Tin::from_string("rain"),
        },
    );
    wait_for_frame(
        &mut rx,
        |frame| matches!(frame, ServerFrame::StateUpdated { key, .. } if key == "custom::weather"),
    )
    .await;

    bridge.send_client_frame(
        1,
        ClientFrame::CreateCharacter {
            data: HashMap::from([("std::name".to_string(), Tin::from_string("Alice"))]),
        },
    );
    let created = wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::CharacterCreated { .. })
    })
    .await;
    let character_id = match created {
        ServerFrame::CharacterCreated { character_id } => character_id,
        _ => unreachable!(),
    };

    bridge.send_client_frame(
        1,
        ClientFrame::MoveCharacter {
            character_id,
            to_scene_id: "tavern".to_string(),
        },
    );
    wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::CharacterMoved { .. })
    })
    .await;

    // Logs are appended to the root scene the client is in, so the append
    // broadcast is observable here. (The moved character already appended a
    // `character-came` entry to the tavern log.)
    bridge.send_client_frame(
        1,
        ClientFrame::AppendLog {
            scene_id: "/".to_string(),
            entry: Some(Tin::from_string("hello")),
        },
    );
    wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::LogEventAppended { .. })
    })
    .await;

    // Issue #199: the room-wide default permission replaced the `done` flag.
    bridge.send_client_frame(
        1,
        ClientFrame::SetDefaultPermission {
            permission: RoomPermissionLevel::Viewer,
        },
    );
    wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::PermissionsUpdated { .. })
    })
    .await;

    // Graceful shutdown persists the final state and reports the head.
    assert!(
        spawner.shutdown_room(ROOM_ID).await,
        "the in-process room must be tracked"
    );
    let head = wait_for_room_head(&db).await;
    manager.remove_room(ROOM_ID).await;

    // A new bridge must restore the room from the head, not start empty.
    let restored = manager
        .get_or_create_room(ROOM_ID, account_id)
        .await
        .expect("the persisted head must restore the room");
    let (room_data, _chunks) = restored
        .export_room(Some(account_id))
        .await
        .expect("export of the restored room");

    assert_eq!(room_data.room_id, ROOM_ID);
    assert_eq!(
        room_data.default_permission,
        RoomPermissionLevel::Viewer,
        "default permission must be restored"
    );
    assert_eq!(room_data.chunk_exponent, 8);
    assert!(room_data.scenes.contains_key("/"));
    let tavern = room_data.scenes.get("tavern").expect("tavern restored");
    assert_eq!(
        tavern
            .states
            .get("custom::weather")
            .unwrap()
            .as_string()
            .unwrap(),
        "rain"
    );
    assert_eq!(tavern.biblio.head, (0, 1));
    // The character was created in `/` and then moved to `tavern`, so the root
    // log holds the `character-gone` event followed by the appended "hello"
    // (Issue #160: creation assigns a scene membership).
    assert_eq!(
        room_data
            .scenes
            .get("/")
            .expect("root scene restored")
            .biblio
            .head,
        (0, 2)
    );
    assert!(tavern.present_characters.contains(&character_id));
    assert_eq!(room_data.character_biblio.len(), 1);
    // Issue #199: room permissions are part of `RoomData`, not `"/"` state.
    assert_eq!(
        room_data.permissions.get(&ADMIN),
        Some(&RoomPermissionLevel::Admin),
        "administrator set must be restored"
    );
    assert!(
        !room_data
            .scenes
            .get("/")
            .unwrap()
            .states
            .contains_key("std::admins"),
        "room permissions must not be mirrored into scene state"
    );

    // The restored banquette serves the same data over the client protocol.
    let mut rx = restored.register_client(2, account_id, session_id).await;
    restored.send_client_frame(2, ClientFrame::ListScenes);
    let scene_list = wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::SceneList { .. })
    })
    .await;
    match scene_list {
        ServerFrame::SceneList { scenes } => {
            assert!(scenes.iter().any(|s| s.id == "/"));
            assert!(scenes.iter().any(|s| s.id == "tavern"));
        }
        _ => unreachable!(),
    }

    restored.send_client_frame(
        2,
        ClientFrame::QueryState {
            scene_id: "tavern".to_string(),
        },
    );
    let snapshot = wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::StateSnapshot { .. })
    })
    .await;
    match snapshot {
        ServerFrame::StateSnapshot { states, .. } => {
            assert_eq!(
                states.get("custom::weather").unwrap().as_string().unwrap(),
                "rain"
            );
        }
        _ => unreachable!(),
    }

    assert_eq!(db.get_room_head(ROOM_ID_HEX).unwrap(), Some(head));

    spawner.shutdown_room(ROOM_ID).await;
}

// spec: alhalte-maitre-d/room-head 0003 heartbeat reports the head to maitre-d
#[tokio::test]
async fn test_heartbeat_updates_room_head() {
    let temp_dir = tempdir().unwrap();
    let db = Database::new_in_memory().unwrap();
    let (account_id, _session_id) = registered_room(&db);

    let cellar = Arc::new(InMemoryCellarSink::new());
    // A 500 ms heartbeat leaves a comfortable window to load the reported head
    // (a subsequent persist supersedes and unregisters the previous one).
    let spawner = Arc::new(
        InProcessBanquetteSpawner::new(temp_dir.path().to_path_buf(), cellar.clone(), 8)
            .with_heartbeat_interval(Duration::from_millis(500)),
    );
    let manager = RoomBridgeManager::new(spawner.clone(), db.clone());

    let bridge = manager
        .get_or_create_room(ROOM_ID, account_id)
        .await
        .expect("new room starts");
    let mut rx = bridge.register_client(1, account_id, 1).await;
    bridge.send_client_frame(
        1,
        ClientFrame::CreateScene {
            scene_id: "garden".to_string(),
            inherit_juices: false,
            name: None,
        },
    );
    wait_for_frame(&mut rx, |frame| {
        matches!(frame, ServerFrame::SceneCreated { .. })
    })
    .await;

    // The reported head is the RoomData tin of the current state and can be restored.
    let mut restored = None;
    for _ in 0..10 {
        let head = wait_for_room_head(&db).await;
        match Room::load_from_cellar(cellar.as_ref(), &head, ROOM_ID).await {
            Ok(room) => {
                restored = Some(room);
                break;
            }
            // A heartbeat may have superseded the head between the read and the load.
            Err(_) => continue,
        }
    }
    let restored = restored.expect("a reported head must be loadable");
    assert!(restored.scene_biblio.contains_key("garden"));

    spawner.shutdown_room(ROOM_ID).await;
}

// spec: alhalte-maitre-d/room-head 0004 a head that cannot be loaded fails loudly
#[tokio::test]
async fn test_missing_or_corrupt_head_is_not_replaced_by_an_empty_room() {
    let temp_dir = tempdir().unwrap();
    let db = Database::new_in_memory().unwrap();
    let (account_id, session_id) = registered_room(&db);

    let cellar = Arc::new(InMemoryCellarSink::new());
    let spawner = Arc::new(InProcessBanquetteSpawner::new(
        temp_dir.path().to_path_buf(),
        cellar.clone(),
        8,
    ));
    let manager = RoomBridgeManager::new(spawner, db.clone());
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
        let _ = axum::serve(listener, app).await;
    });

    // 1. A head that points at a missing tin.
    let missing = [0xABu8; 32];
    db.set_room_head(ROOM_ID_HEX, &missing, 1000).unwrap();
    let err = manager
        .get_or_create_room(ROOM_ID, account_id)
        .await
        .err()
        .expect("missing head must fail the spawn");
    assert!(
        err.to_string().contains("failed to restore room"),
        "clear restore error expected, got: {err}"
    );
    assert_eq!(db.get_room_head(ROOM_ID_HEX).unwrap(), Some(missing));

    // The export route reports a 5xx instead of an empty ZIP.
    let response = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/room/{ROOM_ID_HEX}/export/zip?account_id={account_id}&session_id={session_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("error"), "error body expected, got {body}");
    assert_eq!(db.get_room_head(ROOM_ID_HEX).unwrap(), Some(missing));

    // 2. A head whose tin exists but does not decode into a RoomData tin.
    let garbage = [0xCDu8; 32];
    cellar
        .store_tin(ADMIN, 0, b"garbled room head", &garbage, None)
        .await
        .unwrap();
    db.set_room_head(ROOM_ID_HEX, &garbage, 1001).unwrap();
    let err = manager
        .get_or_create_room(ROOM_ID, account_id)
        .await
        .err()
        .expect("garbled head must fail the spawn");
    assert!(
        err.to_string().contains("failed to restore room"),
        "clear restore error expected, got: {err}"
    );
    assert_eq!(db.get_room_head(ROOM_ID_HEX).unwrap(), Some(garbage));
}

// spec: alhalte-maitre-d/room-head 0005 internal credential + retention end to end
#[tokio::test]
async fn test_internal_service_token_store_and_old_head_retention() {
    let (addr, service, _dir) = spawn_cellar(Some("test-service-token")).await;
    let sink =
        HttpCellarSink::new(format!("http://{addr}")).with_service_token("test-service-token");

    let mut room = Room::new(ROOM_ID, ADMIN, 8).unwrap();
    room.create_scene(ADMIN, "tavern", None).unwrap();
    room.update_scene_state(
        ADMIN,
        "tavern",
        "custom::weather".to_string(),
        Tin::from_string("rain"),
    )
    .unwrap();

    // The store succeeds without any user session; only the service token authorizes it.
    let first_head = room.persist(&sink).await.expect("token store");
    assert_eq!(room.head_tin, Some(first_head));
    assert!(service.download(&first_head, None).await.is_ok());

    // A second persist unregisters the superseded head so GC can reclaim it.
    room.update_scene_state(
        ADMIN,
        "tavern",
        "custom::weather".to_string(),
        Tin::from_string("snow"),
    )
    .unwrap();
    let second_head = room.persist(&sink).await.expect("token store");
    assert_ne!(first_head, second_head);
    assert!(
        service.download(&first_head, None).await.is_err(),
        "the old head reference must be unregistered"
    );
    let stats = service.run_gc().await.unwrap();
    assert!(
        stats.deleted_files >= 1,
        "GC should reclaim the old head file, got {stats:?}"
    );
    // The current head survives GC and restores the latest state.
    assert!(service.download(&second_head, None).await.is_ok());
    let restored = Room::load_from_cellar(&sink, &second_head, ROOM_ID)
        .await
        .expect("restore through the internal credential");
    assert_eq!(
        restored
            .scene_biblio
            .get("tavern")
            .unwrap()
            .state
            .get("custom::weather")
            .unwrap()
            .as_string()
            .unwrap(),
        "snow"
    );

    // A sink without the token is rejected by the cellar verifier (401).
    let untokened = HttpCellarSink::new(format!("http://{addr}"));
    let err = room
        .persist(&untokened)
        .await
        .expect_err("session-less internal write without a token must fail");
    assert!(
        format!("{err}").contains("401") || format!("{err}").contains("Unexpected response status"),
        "expected an authorization failure, got: {err}"
    );
    // The failed store did not advance or unregister the head.
    assert_eq!(room.head_tin, Some(second_head));
    assert!(service.download(&second_head, None).await.is_ok());
}

// spec: alhalte-maitre-d/room-head 0006 process spawner forwards --room-tin
// spec: alhalte-bistrot/secrets 0003 secrets are forwarded as file paths, never argv
#[test]
fn test_process_spawner_passes_room_tin_and_service_token() {
    let token_path = std::path::Path::new("/run/alhalte/secrets/cellar-service-token");
    let spawner = ProcessBanquetteSpawner::new(
        "/usr/local/bin/alhalte-banquette",
        "/run/alhalte",
        "http://127.0.0.1:8081",
        8,
    )
    .with_service_token_file(token_path);
    let head = [0x11u8; 32];
    let socket = std::path::Path::new("/run/alhalte/banquette-1.sock");

    let args = spawner.command_args(1, 42, socket, Some(&head));
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--room-tin" && w[1] == hex::encode(head))
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--service-token-file" && w[1] == token_path.display().to_string())
    );
    assert!(
        !args.iter().any(|arg| arg == "--service-token"),
        "the token value must never be passed as an argument"
    );

    // Without a persisted head no restore flag is passed.
    let args = spawner.command_args(1, 42, socket, None);
    assert!(!args.iter().any(|arg| arg == "--room-tin"));

    // A value-only token is forwarded through the environment, not argv.
    let value_only = ProcessBanquetteSpawner::new(
        "/usr/local/bin/alhalte-banquette",
        "/run/alhalte",
        "http://127.0.0.1:8081",
        8,
    )
    .with_service_token("secret-token");
    let args = value_only.command_args(1, 42, socket, None);
    assert!(
        !args.iter().any(|arg| arg == "secret-token"),
        "the token value must never be passed as an argument"
    );

    // Debug must not leak the token.
    let debug = format!("{value_only:?}");
    assert!(
        !debug.contains("secret-token"),
        "token leaked in Debug: {debug}"
    );
    assert!(debug.contains("<redacted>"));
}

// spec: alhalte-bistrot/service-token 0002 the default Bistrot.toml persists room state
// spec: alhalte-bistrot/secrets 0006 the default config injects the token as a file
#[tokio::test]
async fn test_default_bistrot_config_authorizes_room_persistence() {
    let config = default_bistrot_config();
    let token = config
        .cellar
        .service_token
        .as_deref()
        .expect("the default Bistrot.toml must configure an internal service token")
        .to_string();

    // `nix run .#bistrot` passes the same secret file to both long-lived
    // services, and neither argument list carries the value.
    let token_path = config
        .secrets_dir()
        .join("cellar-service-token")
        .display()
        .to_string();
    let maitre_args = alhalte_bistrot::generate_maitre_d_args(&config);
    let cellar_args = alhalte_bistrot::generate_cellar_args(&config);
    assert_eq!(
        arg_after(&maitre_args, "--service-token-file"),
        Some(token_path.as_str())
    );
    assert_eq!(
        arg_after(&cellar_args, "--service-token-file"),
        Some(token_path.as_str())
    );
    for args in [&maitre_args, &cellar_args] {
        assert!(!args.contains(&token));
        assert!(!args.iter().any(|arg| arg == "--service-token"));
    }

    // maitre-d forwards the injected file path to every spawned banquette.
    let spawner = ProcessBanquetteSpawner::new(
        "/usr/local/bin/alhalte-banquette",
        "/run/alhalte",
        "http://127.0.0.1:8081",
        config.banquette.chunk_size_exponent,
    )
    .with_service_token_file(
        arg_after(&maitre_args, "--service-token-file")
            .expect("maitre-d token file forwarded to banquette"),
    );
    let banquette_args = spawner.command_args(
        ROOM_ID,
        ADMIN,
        std::path::Path::new("/run/alhalte/banquette.sock"),
        None,
    );
    assert_eq!(
        arg_after(&banquette_args, "--service-token-file"),
        Some(token_path.as_str())
    );
    assert!(!banquette_args.contains(&token));

    // With the real session verifier and the default token, the internal
    // RoomData store is accepted (no 401) and restores.
    let (addr, service, _dir) = spawn_cellar(Some(token.as_str())).await;
    let sink = HttpCellarSink::new(format!("http://{addr}")).with_service_token(token.as_str());
    let mut room = Room::new(ROOM_ID, ADMIN, config.banquette.chunk_size_exponent).unwrap();
    room.create_scene(ADMIN, "tavern", None).unwrap();
    let head = room
        .persist(&sink)
        .await
        .expect("the default local configuration must authorize persistence");
    assert!(service.download(&head, None).await.is_ok());
    let restored = Room::load_from_cellar(&sink, &head, ROOM_ID)
        .await
        .expect("the persisted room must restore through the same token");
    assert!(restored.scene_biblio.contains_key("tavern"));
}
