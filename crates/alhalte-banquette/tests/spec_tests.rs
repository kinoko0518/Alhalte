//! Specification compliance tests for `alhalte-banquette`.
//!
//! Each test references the corresponding section of `ARCHITECTURE.md`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tempfile::tempdir;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use alhalte_banquette::cellar_sink::InMemoryCellarSink;
use alhalte_banquette::character::{
    CharacterBiblio, get_character_name, get_character_owner, set_character_name,
    set_character_owner,
};
use alhalte_banquette::error::BanquetteError;
use alhalte_banquette::labels::{
    IDENT_ALHALTE_BANQUETTE, IDENT_ALHALTE_BANQUETTE_LOG_BIBLIO,
    IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG, IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BGM,
    IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
    IDENT_ALHALTE_BANQUETTE_LOG_CHUNK, IDENT_ALHALTE_BANQUETTE_SCENE,
    IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID, IDENT_ALHALTE_MAITRE_D_SESSION_ID, KEY_STD_ADMINS,
    LABEL_ALHALTE_BANQUETTE, LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO,
    LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG, LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BGM,
    LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
    LABEL_ALHALTE_BANQUETTE_LOG_CHUNK, LABEL_ALHALTE_BANQUETTE_SCENE,
    LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, LABEL_ALHALTE_MAITRE_D_SESSION_ID,
};
use alhalte_banquette::log::{
    ChangeBgImgEvent, ChangeBgmEvent, CharacterCameEvent, CharacterGoneEvent, SceneLog,
};
use alhalte_banquette::protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
    WebRtcSignalPayload,
};
use alhalte_banquette::room::{HeartbeatOutcome, Room};
use alhalte_banquette::server::{BanquetteServer, ServerConfig};
use alhalte_banquette::state::SceneState;
use tin::{Tin, label_hash};

#[test]
// spec: alhalte-banquette label definitions and hash identities
fn test_label_definitions_and_hashes() {
    assert_eq!(label_hash(IDENT_ALHALTE_BANQUETTE), LABEL_ALHALTE_BANQUETTE);
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_SCENE),
        LABEL_ALHALTE_BANQUETTE_SCENE
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_LOG_CHUNK),
        LABEL_ALHALTE_BANQUETTE_LOG_CHUNK
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_LOG_BIBLIO),
        LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG),
        LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BGM),
        LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BGM
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME),
        LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE),
        LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID),
        LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID
    );
    assert_eq!(
        label_hash(IDENT_ALHALTE_MAITRE_D_SESSION_ID),
        LABEL_ALHALTE_MAITRE_D_SESSION_ID
    );
}

#[test]
// spec: alhalte-banquette/room-structure 0001 root scene and initial admin
fn test_room_initialization_and_admin_rights() {
    let room = Room::new(1001, 42, 8).expect("Room creation failed");
    assert_eq!(room.room_id, 1001);
    assert!(!room.done);
    assert!(room.scene_biblio.contains_key("/"));
    assert!(room.is_admin(42));
    assert!(!room.is_admin(99));

    // Verify "/" state contains std::admins
    let root = room.scene_biblio.get("/").unwrap();
    let admins_tin = root.state.get(KEY_STD_ADMINS).unwrap();
    let admins: Vec<u64> = postcard::from_bytes(&admins_tin.payload).unwrap();
    assert_eq!(admins, vec![42]);
}

#[test]
// spec: alhalte-banquette/room-structure 0002 only admin can modify "/" state
fn test_admin_only_modifies_root_state() {
    let mut room = Room::new(1001, 42, 8).unwrap();

    // Admin (42) can update root state
    let res = room.update_scene_state(
        42,
        "/",
        "custom::setting".to_string(),
        Tin::from_string("admin_val"),
    );
    assert!(res.is_ok());

    // Non-admin (99) cannot update root state
    let res = room.update_scene_state(
        99,
        "/",
        "custom::setting".to_string(),
        Tin::from_string("hack"),
    );
    assert!(matches!(res, Err(BanquetteError::PermissionDenied)));

    // Non-admin (99) cannot delete root state
    let res = room.delete_scene_state(99, "/", "custom::setting");
    assert!(matches!(res, Err(BanquetteError::PermissionDenied)));

    // Admin can delete root state
    let res = room.delete_scene_state(42, "/", "custom::setting");
    assert!(res.is_ok());
}

#[test]
// spec: alhalte-banquette/room-structure 0003 character biblio and helpers
fn test_character_biblio_and_permissions() {
    let mut biblio = CharacterBiblio::new();

    // Create character
    let mut char1 = HashMap::new();
    set_character_name(&mut char1, "Alice");
    set_character_owner(&mut char1, 100);

    let id1 = biblio.create_character(100, char1);
    assert_eq!(id1, 1);

    let fetched = biblio.get(id1).unwrap();
    assert_eq!(get_character_name(fetched).unwrap(), "Alice");
    assert_eq!(get_character_owner(fetched).unwrap(), 100);

    // Create second character without explicit owner -> auto-set creator as owner
    let mut char2 = HashMap::new();
    set_character_name(&mut char2, "Bob");
    let id2 = biblio.create_character(200, char2);
    assert_eq!(id2, 2);
    assert_eq!(get_character_owner(biblio.get(id2).unwrap()).unwrap(), 200);

    // Non-owner, non-admin cannot update char1
    let mut update_data = HashMap::new();
    set_character_name(&mut update_data, "Alice Hacked");
    let res = biblio.update(id1, 200, false, update_data.clone());
    assert!(matches!(res, Err(BanquetteError::PermissionDenied)));

    // Owner (100) can update char1
    let res = biblio.update(id1, 100, false, update_data.clone());
    assert!(res.is_ok());
    assert_eq!(
        get_character_name(biblio.get(id1).unwrap()).unwrap(),
        "Alice Hacked"
    );

    // Admin (999, is_admin=true) can update char1
    let mut admin_update = HashMap::new();
    set_character_name(&mut admin_update, "Alice by Admin");
    let res = biblio.update(id1, 999, true, admin_update);
    assert!(res.is_ok());
    assert_eq!(
        get_character_name(biblio.get(id1).unwrap()).unwrap(),
        "Alice by Admin"
    );

    // Non-owner cannot delete
    assert!(matches!(
        biblio.delete(id1, 200, false),
        Err(BanquetteError::PermissionDenied)
    ));

    // Admin can delete
    assert!(biblio.delete(id1, 999, true).is_ok());
    assert!(biblio.get(id1).is_none());
}

#[tokio::test]
// spec: alhalte-banquette/access-control 0001 silence for unpermitted non-done queries
async fn test_access_control_silence_when_not_done() {
    let mut room = Room::new(1001, 42, 8).unwrap();
    let cellar = InMemoryCellarSink::new();

    // Create scene "tavern"
    room.create_scene(42, "tavern", None).unwrap();
    room.update_scene_state(
        42,
        "tavern",
        "custom::info".to_string(),
        Tin::from_string("Secret room info"),
    )
    .unwrap();

    // Scene ID itself is public
    let ids = room.list_scene_ids();
    assert!(ids.contains(&"tavern".to_string()));

    // User 99 (not admin, has no characters in tavern) queries scene -> returns None (silence)
    assert!(room.query_scene(99, "tavern").is_none());
    assert!(room.query_state(99, "tavern").is_none());
    assert!(room.query_log_biblio(99, "tavern").is_none());
    assert!(
        room.query_log_chunk(99, "tavern", 0, &cellar)
            .await
            .is_none()
    );

    // Admin (42) queries scene -> returns Some
    assert!(room.query_scene(42, "tavern").is_some());
    assert!(room.query_state(42, "tavern").is_some());

    // Create character owned by user 99 and move into "tavern"
    let mut char_data = HashMap::new();
    set_character_name(&mut char_data, "Warrior");
    let char_id = room.character_biblio.create_character(99, char_data);
    room.move_character(99, char_id, "tavern").unwrap();

    // Now user 99 has character in tavern -> queries succeed!
    assert!(room.query_scene(99, "tavern").is_some());
    assert!(room.query_state(99, "tavern").is_some());
    assert!(room.query_log_biblio(99, "tavern").is_some());
}

#[tokio::test]
// spec: alhalte-banquette/access-control 0002 done flag fully opens all scenes
async fn test_access_control_done_flag() {
    let mut room = Room::new(1001, 42, 8).unwrap();
    let cellar = InMemoryCellarSink::new();

    room.create_scene(42, "secret_vault", None).unwrap();

    // Non-admin cannot access when done == false
    assert!(room.query_scene(99, "secret_vault").is_none());

    // Set done = true (admin only)
    assert!(matches!(
        room.set_done(99, true),
        Err(BanquetteError::PermissionDenied)
    ));
    room.set_done(42, true).unwrap();
    assert!(room.done);

    // Now user 99 can access everything
    assert!(room.query_scene(99, "secret_vault").is_some());
    assert!(room.query_state(99, "secret_vault").is_some());
    assert!(room.query_log_biblio(99, "secret_vault").is_some());
    assert!(
        room.query_log_chunk(99, "secret_vault", 0, &cellar)
            .await
            .is_some()
    );
}

#[tokio::test]
// spec: alhalte-banquette/log 0001 append, edit, delete, rewind operations
async fn test_log_operations() {
    let cellar = InMemoryCellarSink::new();
    let mut log = SceneLog::new(8); // 256 entries capacity
    assert_eq!(log.head(), (0, 0));

    // Append 3 entries
    log.append(Some(Tin::from_string("entry 0")), &cellar, 1, 1)
        .await
        .unwrap();
    log.append(Some(Tin::from_string("entry 1")), &cellar, 1, 1)
        .await
        .unwrap();
    log.append(Some(Tin::from_string("entry 2")), &cellar, 1, 1)
        .await
        .unwrap();
    assert_eq!(log.head(), (0, 3));

    // Read entries
    let entries = log.read_chunk(0, &cellar, Some(1)).await.unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1].as_ref().unwrap().as_string().unwrap(), "entry 1");

    // Edit entry at position 1
    log.edit(0, 1, Tin::from_string("edited 1"), &cellar, 1, 1)
        .await
        .unwrap();
    let entries = log.read_chunk(0, &cellar, Some(1)).await.unwrap();
    assert_eq!(
        entries[1].as_ref().unwrap().as_string().unwrap(),
        "edited 1"
    );

    // Cannot edit at or after head
    let res = log
        .edit(0, 3, Tin::from_string("invalid"), &cellar, 1, 1)
        .await;
    assert!(matches!(
        res,
        Err(BanquetteError::InvalidLogPosition { .. })
    ));

    // Delete entry at position 1 (sets None tombstone)
    log.delete(0, 1, &cellar, 1, 1).await.unwrap();
    let entries = log.read_chunk(0, &cellar, Some(1)).await.unwrap();
    assert!(entries[1].is_none());

    // Rewind head to position 1
    log.rewind(0, 1).unwrap();
    assert_eq!(log.head(), (0, 1));
    let entries = log.read_chunk(0, &cellar, Some(1)).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].as_ref().unwrap().as_string().unwrap(), "entry 0");
}

#[tokio::test]
// spec: alhalte-banquette/log 0002 chunk capacity and cellar upload
async fn test_log_chunk_sealing_to_cellar() {
    let cellar = InMemoryCellarSink::new();
    let mut log = SceneLog::new(2); // 1 << 2 = 4 entries per chunk
    assert_eq!(log.chunk_capacity(), 4);

    // Append 3 entries: head becomes (0, 3), still active chunk
    for i in 0..3 {
        log.append(
            Some(Tin::from_string(&format!("entry {i}"))),
            &cellar,
            10,
            20,
        )
        .await
        .unwrap();
    }
    assert_eq!(log.head(), (0, 3));
    assert_eq!(log.biblio.chunks.len(), 0);

    // 4th entry fills the chunk -> seals and uploads to cellar!
    log.append(Some(Tin::from_string("entry 3")), &cellar, 10, 20)
        .await
        .unwrap();
    assert_eq!(log.head(), (1, 0));
    assert_eq!(log.biblio.chunks.len(), 1);

    // Verify chunk exists in cellar sink
    let tin_id = log.biblio.chunks[0];
    assert!(cellar.contains(&tin_id).await);

    // Verify reading chunk 0 downloads from cellar and restores all 4 items
    let entries = log.read_chunk(0, &cellar, Some(10)).await.unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[3].as_ref().unwrap().as_string().unwrap(), "entry 3");

    // Edit an entry in historical sealed chunk in cellar
    log.edit(0, 2, Tin::from_string("modified sealed"), &cellar, 10, 20)
        .await
        .unwrap();
    let entries = log.read_chunk(0, &cellar, Some(10)).await.unwrap();
    assert_eq!(
        entries[2].as_ref().unwrap().as_string().unwrap(),
        "modified sealed"
    );
}

#[test]
// spec: alhalte-banquette/log 0003 event serialization and duck-typing
fn test_log_event_serialization_and_duck_typing() {
    // 1. ChangeBgImgEvent
    let img_evt = ChangeBgImgEvent {
        image_tin_id: Some([7u8; 32]),
        width: Some(1920),
        height: Some(1080),
    };
    let tin = img_evt.to_tin().unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG);
    let decoded = ChangeBgImgEvent::from_tin(&tin).unwrap();
    assert_eq!(decoded, img_evt);

    let duck = img_evt.to_duck_typed_map();
    let from_duck = ChangeBgImgEvent::from_duck_typed_map(&duck).unwrap();
    assert_eq!(from_duck, img_evt);

    // 2. ChangeBgmEvent (spec: banquette/log change-bgm slot & stop)
    let bgm_evt = ChangeBgmEvent {
        bgm_tin_id: Some([9u8; 32]),
        volume: Some(0.85),
        slot: 2,
        track_index: Some(2),
    };
    let tin = bgm_evt.to_tin().unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BGM);
    let decoded = ChangeBgmEvent::from_tin(&tin).unwrap();
    assert_eq!(decoded, bgm_evt);

    let duck = bgm_evt.to_duck_typed_map();
    let from_duck = ChangeBgmEvent::from_duck_typed_map(&duck).unwrap();
    assert_eq!(from_duck, bgm_evt);

    // Stop BGM with None
    let stop_bgm = ChangeBgmEvent {
        bgm_tin_id: None,
        volume: None,
        slot: 1,
        track_index: None,
    };
    let stop_tin = stop_bgm.to_tin().unwrap();
    let decoded_stop = ChangeBgmEvent::from_tin(&stop_tin).unwrap();
    assert_eq!(decoded_stop.bgm_tin_id, None);
    assert_eq!(decoded_stop.slot, 1);

    // 3. CharacterCameEvent
    let came = CharacterCameEvent {
        character_id: 12345,
    };
    let tin = came.to_tin().unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME);
    let decoded = CharacterCameEvent::from_tin(&tin).unwrap();
    assert_eq!(decoded, came);

    let duck = came.to_duck_typed_map();
    let from_duck = CharacterCameEvent::from_duck_typed_map(&duck).unwrap();
    assert_eq!(from_duck, came);

    // 4. CharacterGoneEvent
    let gone = CharacterGoneEvent {
        character_id: 12345,
    };
    let tin = gone.to_tin().unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE);
    let decoded = CharacterGoneEvent::from_tin(&tin).unwrap();
    assert_eq!(decoded, gone);

    let duck = gone.to_duck_typed_map();
    let from_duck = CharacterGoneEvent::from_duck_typed_map(&duck).unwrap();
    assert_eq!(from_duck, gone);
}

#[test]
// spec: alhalte-banquette/state 0001 4kib size restriction and namespace validation
fn test_state_size_limit_and_namespaces() {
    let mut state = SceneState::new();

    // Valid key and valid size (< 4KiB)
    let ok_val = Tin::from_string("hello world");
    assert!(state.update("std::bgm".to_string(), ok_val, false).is_ok());

    // Key without :: separator -> InvalidStateKey error
    let bad_key_val = Tin::from_string("bad key");
    let res = state.update("nokey".to_string(), bad_key_val, false);
    assert!(matches!(res, Err(BanquetteError::InvalidStateKey(_))));

    // Value exceeding 4096 bytes -> StateValueTooLarge error
    // Plaintext = label(8B) + payload, so payload = 4089 gives 4097 bytes > 4096
    let big_payload = vec![0x42; 4089];
    let big_tin = Tin::new(12345, big_payload);
    assert_eq!(big_tin.to_plaintext().len(), 4097);
    let res = state.update("custom::big".to_string(), big_tin, false);
    assert!(matches!(
        res,
        Err(BanquetteError::StateValueTooLarge {
            size: 4097,
            max: 4096
        })
    ));

    // Exactly 4096 bytes is allowed
    let exact_payload = vec![0x42; 4088];
    let exact_tin = Tin::new(12345, exact_payload);
    assert_eq!(exact_tin.to_plaintext().len(), 4096);
    assert!(
        state
            .update("custom::exact".to_string(), exact_tin, false)
            .is_ok()
    );

    // std::admins requires admin
    let admin_val = Tin::from_string("admins");
    assert!(matches!(
        state.update(KEY_STD_ADMINS.to_string(), admin_val.clone(), false),
        Err(BanquetteError::ReservedKeyPermissionDenied(_))
    ));
    assert!(
        state
            .update(KEY_STD_ADMINS.to_string(), admin_val, true)
            .is_ok()
    );
}

#[tokio::test]
// spec: alhalte-banquette/lifecycle 0001 scene unloading and on-demand reload
async fn test_scene_unload_and_restore() {
    let cellar = InMemoryCellarSink::new();
    let mut room = Room::new(1001, 42, 8).unwrap();

    room.create_scene(42, "dungeon", None).unwrap();
    room.update_scene_state(
        42,
        "dungeon",
        "custom::lighting".to_string(),
        Tin::from_string("torchlight"),
    )
    .unwrap();

    // Client 1 enters dungeon
    room.client_enter_scene(1, 42, "dungeon", &cellar)
        .await
        .unwrap();
    let scene = room.scene_biblio.get("dungeon").unwrap();
    assert!(scene.is_loaded);
    assert_eq!(scene.active_clients.len(), 1);

    // Client 1 leaves dungeon -> active clients drops to 0 -> unloaded!
    room.client_leave_scene(1, 42, 100, "dungeon", &cellar)
        .await
        .unwrap();

    let scene = room.scene_biblio.get("dungeon").unwrap();
    assert!(!scene.is_loaded);
    assert!(scene.persisted_tin_id.is_some());

    // Client 2 enters dungeon -> reloads on-demand!
    room.client_enter_scene(2, 42, "dungeon", &cellar)
        .await
        .unwrap();
    let scene = room.scene_biblio.get("dungeon").unwrap();
    assert!(scene.is_loaded);
    assert_eq!(
        scene
            .state
            .get("custom::lighting")
            .unwrap()
            .as_string()
            .unwrap(),
        "torchlight"
    );
}

#[tokio::test]
// spec: alhalte-banquette/lifecycle 0002 periodic heartbeat and exit after 3 missed ticks
async fn test_heartbeat_and_idle_exit() {
    let cellar = InMemoryCellarSink::new();
    let mut room = Room::new(1001, 42, 8).unwrap();

    // Initial state
    assert_eq!(room.missed_heartbeats, 0);

    // 1st tick with client activity: reset missed to 0
    room.record_client_activity();
    let outcome = room.step_heartbeat(&cellar, 42, 1).await.unwrap();
    assert_eq!(outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 0);

    // 1st idle tick
    let outcome = room.step_heartbeat(&cellar, 42, 1).await.unwrap();
    assert_eq!(outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 1);

    // 2nd idle tick
    let outcome = room.step_heartbeat(&cellar, 42, 1).await.unwrap();
    assert_eq!(outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 2);

    // Activity arrives! Resets counter
    room.record_client_activity();
    let outcome = room.step_heartbeat(&cellar, 42, 1).await.unwrap();
    assert_eq!(outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 0);

    // Now 3 consecutive idle ticks
    let _ = room.step_heartbeat(&cellar, 42, 1).await.unwrap(); // tick 1
    let _ = room.step_heartbeat(&cellar, 42, 1).await.unwrap(); // tick 2
    let outcome = room.step_heartbeat(&cellar, 42, 1).await.unwrap(); // tick 3
    assert_eq!(outcome, HeartbeatOutcome::ShouldExit);
    assert_eq!(room.missed_heartbeats, 3);
}

#[test]
// spec: alhalte-banquette whole room serialization and deserialization
fn test_whole_room_tin_serialization() {
    let mut room = Room::new(1001, 42, 8).unwrap();
    room.create_scene(42, "forest", None).unwrap();
    room.update_scene_state(
        42,
        "forest",
        "custom::weather".to_string(),
        Tin::from_string("rain"),
    )
    .unwrap();

    let tin = room.to_tin().unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_BANQUETTE);

    let restored = Room::from_tin(&tin).unwrap();
    assert_eq!(restored.room_id, 1001);
    assert!(restored.scene_biblio.contains_key("/"));
    assert!(restored.scene_biblio.contains_key("forest"));
    let forest = restored.scene_biblio.get("forest").unwrap();
    assert_eq!(
        forest
            .state
            .get("custom::weather")
            .unwrap()
            .as_string()
            .unwrap(),
        "rain"
    );
}

#[tokio::test]
// spec: alhalte-banquette/protocol 0001 unix domain socket integration
async fn test_unix_socket_server_integration() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("banquette.sock");

    let cellar = Arc::new(InMemoryCellarSink::new());
    let room = Room::new(2002, 10, 8).unwrap();

    let config = ServerConfig {
        socket_path: sock_path.clone(),
        heartbeat_interval: Duration::from_secs(3600), // large interval
        admin_account: 10,
        system_session_id: 1,
    };

    let server = BanquetteServer::new(config, room, cellar);
    let server_task = tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Wait until socket file exists
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect as maitre-d
    let client_stream = UnixStream::connect(&sock_path).await.unwrap();
    let (read_half, write_half) = client_stream.into_split();
    let mut stream = Framed::new(read_half, ProtocolCodec::<BanquetteToMaitreD>::new());
    let mut client_sink = Framed::new(write_half, ProtocolCodec::<MaitreDToBanquette>::new());

    client_sink
        .send(MaitreDToBanquette::ClientConnected {
            client_id: 100,
            account_id: 10,
            session_id: 1,
        })
        .await
        .unwrap();

    // Send Ping
    client_sink.send(MaitreDToBanquette::Ping).await.unwrap();

    // Send ClientFrame::ListScenes
    client_sink
        .send(MaitreDToBanquette::ClientMessage {
            client_id: 100,
            frame: ClientFrame::ListScenes,
        })
        .await
        .unwrap();

    // Read responses
    let mut received_pong = false;
    let mut received_list = false;

    for _ in 0..2 {
        if let Some(Ok(frame)) = stream.next().await {
            match frame {
                BanquetteToMaitreD::Pong => received_pong = true,
                BanquetteToMaitreD::SendToClient { client_id, frame } => {
                    assert_eq!(client_id, 100);
                    if let ServerFrame::SceneList { scenes } = frame {
                        assert!(scenes.contains(&"/".to_string()));
                        received_list = true;
                    }
                }
                _ => {}
            }
        }
    }

    assert!(received_pong);
    assert!(received_list);

    // Test WebRTC signaling relay between client 100 and client 200
    client_sink
        .send(MaitreDToBanquette::ClientMessage {
            client_id: 100,
            frame: ClientFrame::WebRtcSignal {
                to_client_id: 200,
                payload: WebRtcSignalPayload::Offer("sdp_offer_str".to_string()),
            },
        })
        .await
        .unwrap();

    if let Some(Ok(BanquetteToMaitreD::SendToClient { client_id, frame })) = stream.next().await {
        assert_eq!(client_id, 200);
        if let ServerFrame::WebRtcSignal {
            from_client_id,
            payload,
        } = frame
        {
            assert_eq!(from_client_id, 100);
            assert_eq!(
                payload,
                WebRtcSignalPayload::Offer("sdp_offer_str".to_string())
            );
        } else {
            panic!("Unexpected frame: {:?}", frame);
        }
    }

    server_task.abort();
}
