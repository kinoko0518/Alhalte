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

use alhalte_banquette::cellar_sink::{INTERNAL_WRITE_SESSION_ID, InMemoryCellarSink};
use alhalte_banquette::character::{
    CharacterBiblio, get_character_name, get_character_owner, set_character_name,
    set_character_owner,
};
use alhalte_banquette::error::BanquetteError;
use alhalte_banquette::labels::{
    IDENT_ALHALTE_BANQUETTE, IDENT_ALHALTE_BANQUETTE_LOG_BIBLIO,
    IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME, IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE,
    IDENT_ALHALTE_BANQUETTE_LOG_CHUNK, IDENT_ALHALTE_BANQUETTE_SCENE,
    IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID, IDENT_ALHALTE_MAITRE_D_SESSION_ID, LABEL_ALHALTE_BANQUETTE,
    LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO, LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME,
    LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE, LABEL_ALHALTE_BANQUETTE_LOG_CHUNK,
    LABEL_ALHALTE_BANQUETTE_SCENE, LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID,
    LABEL_ALHALTE_MAITRE_D_SESSION_ID,
};
use alhalte_banquette::log::{CharacterCameEvent, CharacterGoneEvent, SceneLog};
use alhalte_banquette::protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
    WebRtcSignalPayload,
};
use alhalte_banquette::room::{HeartbeatOutcome, Room, RoomPermissionLevel, room_head_alias};
use alhalte_banquette::server::{BanquetteServer, ServerConfig};
use alhalte_banquette::state::SceneState;
use alhalte_cellar::SYSTEM_OWNER;
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
    assert_eq!(room.default_permission, RoomPermissionLevel::Denied);
    assert!(room.scene_biblio.contains_key("/"));
    assert!(room.is_admin(42));
    assert_eq!(room.get_account_permission(42), RoomPermissionLevel::Admin);
    // An unknown account falls back to the closed default.
    assert_eq!(room.get_account_permission(99), RoomPermissionLevel::Denied);
    assert!(!room.is_admin(99));
    room.validate().expect("a fresh room is valid");

    // The permission table lives in `RoomData`, not in `"/"` state.
    let root = room.scene_biblio.get("/").unwrap();
    assert!(root.state.get("std::admins").is_none());
    assert_eq!(room.admin_accounts(), vec![42]);
    assert_eq!(room.accessible_scenes_of(42), ["/".to_string()]);
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
// spec: alhalte-banquette/access-control 0001 silence for unpermitted queries
async fn test_access_control_silence_for_denied_accounts() {
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

    // User 99 is Denied by the closed default permission -> silence.
    assert_eq!(room.get_account_permission(99), RoomPermissionLevel::Denied);
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

    // A participant only sees the scenes an administrator moved it into: the
    // character owner no longer inherits read access by owning a character.
    room.set_member_permission(42, 99, RoomPermissionLevel::Participant)
        .unwrap();
    assert!(room.query_scene(99, "tavern").is_none());
    assert!(room.query_state(99, "/").is_some());

    let mut char_data = HashMap::new();
    set_character_name(&mut char_data, "Warrior");
    let char_id = room.create_character(99, char_data).unwrap();
    room.assign_character_to_scene(char_id, "/").unwrap();

    // Only an administrator moves characters, and the destination becomes an
    // accessible scene for the participant owner.
    assert!(matches!(
        room.move_character(99, char_id, "tavern"),
        Err(BanquetteError::PermissionDenied)
    ));
    room.move_character(42, char_id, "tavern").unwrap();
    assert_eq!(
        room.accessible_scenes_of(99),
        ["/".to_string(), "tavern".to_string()]
    );

    // Now user 99 participates in tavern -> queries succeed!
    assert!(room.query_scene(99, "tavern").is_some());
    assert!(room.query_state(99, "tavern").is_some());
    assert!(room.query_log_biblio(99, "tavern").is_some());
}

#[tokio::test]
// spec: alhalte-banquette/access-control 0002 viewer reads every scene but writes none
async fn test_access_control_viewer_reads_everything() {
    let mut room = Room::new(1001, 42, 8).unwrap();
    let cellar = InMemoryCellarSink::new();

    room.create_scene(42, "secret_vault", None).unwrap();

    // User 99 is Denied until an administrator admits it.
    assert!(room.query_scene(99, "secret_vault").is_none());

    // Raising the default permission is administrator-only.
    assert!(matches!(
        room.set_default_permission(99, RoomPermissionLevel::Viewer),
        Err(BanquetteError::PermissionDenied)
    ));
    room.set_default_permission(42, RoomPermissionLevel::Viewer)
        .unwrap();
    assert_eq!(room.default_permission, RoomPermissionLevel::Viewer);

    // Now user 99 can read everything, but never write.
    assert!(room.query_scene(99, "secret_vault").is_some());
    assert!(room.query_state(99, "secret_vault").is_some());
    assert!(room.query_log_biblio(99, "secret_vault").is_some());
    assert!(
        room.query_log_chunk(99, "secret_vault", 0, &cellar)
            .await
            .is_some()
    );
    assert!(matches!(
        room.update_scene_state(
            99,
            "secret_vault",
            "custom::info".to_string(),
            Tin::from_string("hack"),
        ),
        Err(BanquetteError::PermissionDenied)
    ));
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
    // 1. CharacterCameEvent
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

    // 2. CharacterGoneEvent
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
    assert!(state.update("std::custom".to_string(), ok_val).is_ok());

    // Key without :: separator -> InvalidStateKey error
    let bad_key_val = Tin::from_string("bad key");
    let res = state.update("nokey".to_string(), bad_key_val);
    assert!(matches!(res, Err(BanquetteError::InvalidStateKey(_))));

    // Value exceeding 4096 bytes -> StateValueTooLarge error
    // Plaintext = label(8B) + payload, so payload = 4089 gives 4097 bytes > 4096
    let big_payload = vec![0x42; 4089];
    let big_tin = Tin::new(12345, big_payload);
    assert_eq!(big_tin.to_plaintext().len(), 4097);
    let res = state.update("custom::big".to_string(), big_tin);
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
    assert!(state.update("custom::exact".to_string(), exact_tin).is_ok());

    // No state key is reserved for administrators any more: room permissions
    // live in `RoomData.permissions`, so a `std::` key is writable like any
    // other once the caller passed the scene write check (Issue #199).
    let std_val = Tin::from_string("value");
    assert!(state.update("std::custom".to_string(), std_val).is_ok());
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
    let result = room.step_heartbeat(&cellar).await.unwrap();
    assert_eq!(result.outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 0);

    // 1st idle tick
    let result = room.step_heartbeat(&cellar).await.unwrap();
    assert_eq!(result.outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 1);

    // 2nd idle tick
    let result = room.step_heartbeat(&cellar).await.unwrap();
    assert_eq!(result.outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 2);

    // Activity arrives! Resets counter
    room.record_client_activity();
    let result = room.step_heartbeat(&cellar).await.unwrap();
    assert_eq!(result.outcome, HeartbeatOutcome::Continue);
    assert_eq!(room.missed_heartbeats, 0);

    // Now 3 consecutive idle ticks
    let _ = room.step_heartbeat(&cellar).await.unwrap(); // tick 1
    let _ = room.step_heartbeat(&cellar).await.unwrap(); // tick 2
    let result = room.step_heartbeat(&cellar).await.unwrap(); // tick 3
    assert_eq!(result.outcome, HeartbeatOutcome::ShouldExit);
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
                        assert!(scenes.iter().any(|s| s.id == "/"));
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

// ---------------------------------------------------------------------------
// Issue #157: room persistence, restore, and internal cellar credentials
// ---------------------------------------------------------------------------

use alhalte_banquette::cellar_sink::CellarSink;
use alhalte_banquette::error::BanquetteError as PersistenceError;
use tin::TinId;

/// Cellar sink that records the credentials of internal writes.
///
/// `fail_store` makes every store fail so heartbeat failure handling can be
/// observed.
#[derive(Default)]
struct RecordingCellarSink {
    inner: InMemoryCellarSink,
    stores: std::sync::Mutex<Vec<(u64, u64, TinId)>>,
    unregisters: std::sync::Mutex<Vec<TinId>>,
    fail_store: bool,
}

impl RecordingCellarSink {
    fn failing() -> Self {
        Self {
            fail_store: true,
            ..Self::default()
        }
    }

    fn stores(&self) -> Vec<(u64, u64, TinId)> {
        self.stores.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl CellarSink for RecordingCellarSink {
    async fn store_tin(
        &self,
        owner: u64,
        session_id: u64,
        tin_bytes: &[u8],
        tin_id: &TinId,
        wrapped_key: Option<Vec<u8>>,
    ) -> Result<alhalte_cellar::StoreResult, PersistenceError> {
        self.stores
            .lock()
            .unwrap()
            .push((owner, session_id, *tin_id));
        if self.fail_store {
            return Err(PersistenceError::Cellar(
                "simulated store failure".to_string(),
            ));
        }
        self.inner
            .store_tin(owner, session_id, tin_bytes, tin_id, wrapped_key)
            .await
    }

    async fn download_tin(
        &self,
        tin_id: &TinId,
        owner: Option<u64>,
    ) -> Result<Vec<u8>, PersistenceError> {
        self.inner.download_tin(tin_id, owner).await
    }

    async fn unregister_tin(
        &self,
        owner: u64,
        session_id: u64,
        tin_id: &TinId,
    ) -> Result<bool, PersistenceError> {
        self.unregisters.lock().unwrap().push(*tin_id);
        self.inner.unregister_tin(owner, session_id, tin_id).await
    }

    async fn get_alias(&self, name: &str) -> Result<Option<TinId>, PersistenceError> {
        self.inner.get_alias(name).await
    }

    async fn set_alias(&self, name: &str, tin_id: &TinId) -> Result<(), PersistenceError> {
        self.inner.set_alias(name, tin_id).await
    }
}

/// Builds a room containing scenes, state, characters, log entries, and permissions.
async fn populated_room(cellar: &dyn CellarSink) -> Room {
    let room_id = 0xabcd_ef01_2345_6789u64;
    let mut room = Room::new(room_id, 42, 8).unwrap();
    room.create_scene(42, "tavern", Some("/")).unwrap();
    room.update_scene_state(
        42,
        "tavern",
        "custom::weather".to_string(),
        Tin::from_string("rain"),
    )
    .unwrap();
    room.update_scene_state(42, "/", "std::asset".to_string(), Tin::from_string("bg"))
        .unwrap();
    let character: alhalte_banquette::Character =
        HashMap::from([("std::name".to_string(), Tin::from_string("Alice"))]);
    let character_id = room.create_character(42, character).unwrap();
    room.move_character(42, character_id, "tavern").unwrap();
    room.append_log(42, 100, "tavern", Some(Tin::from_string("hello")), cellar)
        .await
        .unwrap();
    room.append_log(42, 100, "tavern", Some(Tin::from_string("world")), cellar)
        .await
        .unwrap();
    room.set_default_permission(42, RoomPermissionLevel::Viewer)
        .unwrap();
    room.set_member_permission(42, 77, RoomPermissionLevel::Participant)
        .unwrap();
    room.validate().expect("populated room is valid");
    room
}

// spec: alhalte-banquette/persistence 0008 internal room writes use the system owner label
#[tokio::test]
async fn test_persist_uses_the_system_owner_label() {
    let cellar = RecordingCellarSink::default();
    let mut room = populated_room(&cellar).await;

    let first = room.persist(&cellar).await.unwrap();
    let stores = cellar.stores();
    assert_eq!(stores.len(), 1);
    assert_eq!(
        stores[0].0,
        alhalte_cellar::SYSTEM_OWNER,
        "room state must not be labelled with the room administrator"
    );
    assert_eq!(stores[0].1, INTERNAL_WRITE_SESSION_ID);
    assert_eq!(stores[0].2, first);

    // The superseded head is unregistered under the same system owner label.
    room.update_scene_state(
        42,
        "tavern",
        "custom::weather".to_string(),
        Tin::from_string("snow"),
    )
    .unwrap();
    let second = room.persist(&cellar).await.unwrap();
    assert_ne!(first, second);
    assert_eq!(cellar.stores().len(), 2);
    assert_eq!(cellar.stores()[1].0, alhalte_cellar::SYSTEM_OWNER);
    assert_eq!(cellar.unregisters.lock().unwrap().as_slice(), &[first]);
}

// spec: alhalte-banquette/persistence 0001 persist and restore the whole room state
#[tokio::test]
async fn test_persist_and_restore_whole_room() {
    let cellar = InMemoryCellarSink::new();
    let mut room = populated_room(&cellar).await;
    let room_id = room.room_id;

    let head = room.persist(&cellar).await.unwrap();
    assert_eq!(room.head_tin, Some(head));
    assert!(cellar.contains(&head).await);

    let restored = Room::load_from_cellar(&cellar, &head, room_id)
        .await
        .unwrap();
    assert_eq!(restored.room_id, room_id);
    assert_eq!(restored.default_permission, RoomPermissionLevel::Viewer);
    assert_eq!(restored.chunk_exponent, 8);
    assert_eq!(restored.head_tin, Some(head));
    // permissions survive
    assert!(restored.is_admin(42));
    assert_eq!(
        restored.get_account_permission(77),
        RoomPermissionLevel::Participant
    );
    assert_eq!(
        restored.get_account_permission(99),
        RoomPermissionLevel::Viewer
    );
    assert_eq!(restored.accessible_scenes_of(77), ["/".to_string()]);
    // scenes and state survive
    let tavern = restored
        .scene_biblio
        .get("tavern")
        .expect("tavern restored");
    assert_eq!(
        tavern
            .state
            .get("custom::weather")
            .unwrap()
            .as_string()
            .unwrap(),
        "rain"
    );
    assert_eq!(
        restored
            .scene_biblio
            .get("/")
            .unwrap()
            .state
            .get("std::asset")
            .unwrap()
            .as_string()
            .unwrap(),
        "bg"
    );
    // log biblio survives
    assert_eq!(tavern.log.biblio.head, (0, 2));
    // characters and ownership survive
    let character = restored
        .character_biblio
        .get(1)
        .expect("character restored");
    assert_eq!(get_character_owner(character), Some(42));
    assert_eq!(get_character_name(character).as_deref(), Some("Alice"));
    assert!(tavern.present_characters.contains(&1));
    // export of the restored room also exposes the data
    let (data, _chunks) = restored.export_snapshot(&cellar, Some(42)).await.unwrap();
    assert_eq!(data.default_permission, RoomPermissionLevel::Viewer);
    assert_eq!(
        data.permissions.get(&77),
        Some(&RoomPermissionLevel::Participant)
    );
    assert!(data.scenes.contains_key("tavern"));
    assert_eq!(data.character_biblio.len(), 1);
}

// spec: alhalte-banquette/persistence 0003 scene membership survives reload (Issue #160)
#[tokio::test]
async fn test_character_scene_membership_survives_persist_and_restore() {
    let cellar = InMemoryCellarSink::new();
    let mut room = Room::new(4242, 42, 8).unwrap();

    let mut data: HashMap<String, Tin> = HashMap::new();
    set_character_name(&mut data, "Alice");
    let character_id = room.character_biblio.create_character(42, data);
    let affected = room
        .assign_character_to_scene(character_id, "/")
        .expect("root scene exists");
    assert_eq!(affected, vec!["/".to_string()]);

    // The protocol summary exposes the full membership list, not only a count.
    let summary = room.scene_summary("/").unwrap();
    assert_eq!(summary.present_character_count, 1);
    assert_eq!(summary.present_characters, vec![character_id]);

    let head = room.persist(&cellar).await.unwrap();
    let restored = Room::load_from_cellar(&cellar, &head, 4242).await.unwrap();

    let root = restored.scene_biblio.get("/").expect("root restored");
    assert!(root.present_characters.contains(&character_id));
    assert_eq!(
        restored.scene_summary("/").unwrap().present_characters,
        vec![character_id]
    );

    // Deleting the character removes the membership and the piece state.
    let mut room = restored;
    room.update_scene_state(
        42,
        "/",
        format!("piece::{character_id}::placed"),
        Tin::from_bool(true),
    )
    .unwrap();
    room.character_biblio
        .delete(character_id, 42, true)
        .unwrap();
    assert_eq!(
        room.remove_character_from_scenes(character_id),
        vec!["/".to_string()]
    );
    assert!(
        room.scene_summary("/")
            .unwrap()
            .present_characters
            .is_empty()
    );
    assert_eq!(
        room.clear_character_piece_state(character_id),
        vec![("/".to_string(), format!("piece::{character_id}::placed"))]
    );
    assert!(
        room.scene_biblio
            .get("/")
            .unwrap()
            .state
            .get(&format!("piece::{character_id}::placed"))
            .is_none()
    );
}

// spec: alhalte-banquette/persistence 0002 superseded head is unregistered (retention)
#[tokio::test]
async fn test_superseded_head_is_unregistered() {
    let cellar = InMemoryCellarSink::new();
    let mut room = populated_room(&cellar).await;

    let first_head = room.persist(&cellar).await.unwrap();
    room.update_scene_state(
        42,
        "tavern",
        "custom::weather".to_string(),
        Tin::from_string("snow"),
    )
    .unwrap();
    let second_head = room.persist(&cellar).await.unwrap();

    assert_ne!(first_head, second_head);
    assert_eq!(room.head_tin, Some(second_head));
    assert!(
        !cellar.contains(&first_head).await,
        "superseded head must be unregistered so cellar GC can reclaim it"
    );
    assert!(
        cellar.contains(&second_head).await,
        "the current head must never be unregistered"
    );

    // Re-persisting must never unregister the freshly stored head. `RoomData`
    // contains `HashMap`s, so an unchanged room can serialize to a different
    // content-addressed tin id (postcard map ordering); in that case the
    // superseded head is the one that gets reclaimed.
    let third_head = room.persist(&cellar).await.unwrap();
    assert_eq!(room.head_tin, Some(third_head));
    assert!(cellar.contains(&third_head).await);
    if third_head != second_head {
        assert!(!cellar.contains(&second_head).await);
    }
}

// spec: alhalte-banquette/persistence 0003 missing or corrupt head fails loudly
#[tokio::test]
async fn test_load_from_cellar_rejects_missing_or_corrupt_head() {
    let cellar = InMemoryCellarSink::new();
    let room_id = 1001u64;

    // Missing tin.
    let missing = [7u8; 32];
    let err = Room::load_from_cellar(&cellar, &missing, room_id).await;
    assert!(err.is_err(), "missing head must not produce an empty room");

    // Garbled payload stored under the head tin id.
    let garbage = [9u8; 32];
    cellar
        .store_tin(42, 0, b"not a tin at all", &garbage, None)
        .await
        .unwrap();
    let err = Room::load_from_cellar(&cellar, &garbage, room_id).await;
    assert!(err.is_err(), "garbled head must not produce an empty room");

    // A valid room tin for a different room id.
    let mut other = Room::new(999, 42, 8).unwrap();
    let other_head = other.persist(&cellar).await.unwrap();
    let err = Room::load_from_cellar(&cellar, &other_head, room_id).await;
    assert!(
        matches!(err, Err(PersistenceError::RoomIdMismatch { .. })),
        "room id mismatch must be reported, got {err:?}"
    );

    // Valid room id but the label is not a RoomData tin.
    let wrong_label = Tin::from_string("not room data");
    let (bytes, wrong_label_id) = wrong_label
        .encode_plaintext(tin::Compression::Zstd)
        .unwrap();
    cellar
        .store_tin(42, 0, &bytes, &wrong_label_id, None)
        .await
        .unwrap();
    let err = Room::load_from_cellar(&cellar, &wrong_label_id, room_id).await;
    assert!(err.is_err(), "wrong label must not produce an empty room");
}

// spec: alhalte-banquette/persistence 0004 heartbeat reports only successful stores
#[tokio::test]
async fn test_heartbeat_reports_persisted_tin_and_uses_internal_credential() {
    let cellar = RecordingCellarSink::default();
    let mut room = Room::new(1001, 42, 8).unwrap();
    room.record_client_activity();

    let result = room.step_heartbeat(&cellar).await.unwrap();
    assert_eq!(result.outcome, HeartbeatOutcome::Continue);
    let tin_id = result.persisted_tin.expect("successful store reported");
    assert_eq!(room.head_tin, Some(tin_id));

    // Internal writes use the system owner label and never a synthetic user session.
    let stores = cellar.stores();
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].0, SYSTEM_OWNER);
    assert_eq!(
        stores[0].1, 0,
        "internal room writes must not impersonate a user session"
    );

    // A failed store is logged and reported as no persisted tin.
    let failing = RecordingCellarSink::failing();
    let mut failing_room = Room::new(1002, 42, 8).unwrap();
    failing_room.record_client_activity();
    let result = failing_room
        .step_heartbeat(&failing)
        .await
        .expect("store failure is non-fatal");
    assert_eq!(result.persisted_tin, None);
    assert_eq!(failing_room.head_tin, None);
}

// spec: alhalte-banquette/persistence 0005 heartbeat moves the room head alias
//
// Issue #192 moved the head pointer from maitre-d's `room_heads` to cellar's
// generic alias, so a persist is observed in cellar instead of a
// `RoomPersisted` protocol frame.
#[tokio::test]
async fn test_heartbeat_moves_the_room_head_alias() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("banquette.sock");
    let cellar = Arc::new(InMemoryCellarSink::new());
    let room_id = 2002;
    let alias = room_head_alias(room_id);
    let cellar_sink: Arc<dyn CellarSink> = Arc::clone(&cellar) as Arc<dyn CellarSink>;

    let server = BanquetteServer::new(
        ServerConfig {
            socket_path: sock_path.clone(),
            heartbeat_interval: Duration::from_millis(50),
        },
        Room::new(room_id, 10, 8).unwrap(),
        Arc::clone(&cellar_sink),
    );
    let server_task = tokio::spawn(async move { server.run().await });

    // Keep a client connected so the room is not considered idle and does not
    // shut down (and persist) before the heartbeat path has run.
    for _ in 0..400 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let stream = UnixStream::connect(&sock_path).await.unwrap();
    let (read_half, write_half) = stream.into_split();
    let mut reader = Framed::new(read_half, ProtocolCodec::<BanquetteToMaitreD>::new());
    let mut writer = Framed::new(write_half, ProtocolCodec::<MaitreDToBanquette>::new());
    writer
        .send(MaitreDToBanquette::ClientConnected {
            client_id: 1,
            account_id: 10,
            session_id: 1,
        })
        .await
        .unwrap();
    // Waiting for the `Pong` proves the connection is being handled, so the
    // client is registered before the next heartbeat tick.
    writer.send(MaitreDToBanquette::Ping).await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), reader.next()).await {
            Ok(Some(Ok(BanquetteToMaitreD::Pong))) => break,
            Ok(Some(Ok(_))) => continue,
            other => panic!("expected Pong before the heartbeat: {other:?}"),
        }
    }

    let mut head = None;
    for _ in 0..100 {
        if let Some(tin_id) = cellar.get_alias(&alias).await.unwrap() {
            head = Some(tin_id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let tin_id = head.expect("the heartbeat must move the room head alias");
    assert!(cellar.contains(&tin_id).await);

    // The aliased head is the restore point and decodes back into the room.
    let restored = Room::load_from_cellar(cellar_sink.as_ref(), &tin_id, room_id)
        .await
        .expect("the aliased head must restore");
    assert!(restored.scene_biblio.contains_key("/"));

    server_task.abort();
}

// spec: alhalte-banquette/persistence 0006 graceful shutdown persists once more
#[tokio::test]
async fn test_graceful_shutdown_moves_the_room_head_alias() {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("banquette.sock");
    let cellar = Arc::new(InMemoryCellarSink::new());
    let room_id = 2003;

    let mut room = Room::new(room_id, 10, 8).unwrap();
    room.set_default_permission(10, RoomPermissionLevel::Viewer)
        .unwrap();

    let server = BanquetteServer::new(
        ServerConfig {
            socket_path: sock_path.clone(),
            heartbeat_interval: Duration::from_secs(3600),
        },
        room,
        Arc::clone(&cellar) as Arc<dyn CellarSink>,
    );
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let server_task = tokio::spawn(async move { server.run_until_shutdown(shutdown_rx).await });

    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_tx.send(true).unwrap();
    let _ = server_task.await.unwrap();

    let tin_id = cellar
        .get_alias(&room_head_alias(room_id))
        .await
        .unwrap()
        .expect("graceful shutdown must move the final head alias");
    assert!(cellar.contains(&tin_id).await);

    let restored = Room::load_from_cellar(&*cellar, &tin_id, room_id)
        .await
        .unwrap();
    assert_eq!(restored.default_permission, RoomPermissionLevel::Viewer);
}

// spec: alhalte-banquette/persistence 0010 banquette restores itself from the head alias
#[tokio::test]
async fn test_load_or_create_restores_from_the_head_alias() {
    let cellar = InMemoryCellarSink::new();
    let room_id = 2010u64;
    let alias = room_head_alias(room_id);
    assert_eq!(alias, "room/00000000000007da/head");

    // With no alias the room was never persisted, so it starts new.
    let mut room = Room::load_or_create(&cellar, room_id, 42, 8).await.unwrap();
    assert_eq!(room.room_id, room_id);
    assert!(room.is_admin(42));
    assert_eq!(room.head_tin, None);
    assert_eq!(cellar.get_alias(&alias).await.unwrap(), None);

    // A successful persist moves the alias to the stored head.
    room.create_scene(42, "tavern", None).unwrap();
    let head = room.persist(&cellar).await.unwrap();
    assert_eq!(room.head_tin, Some(head));
    assert_eq!(cellar.get_alias(&alias).await.unwrap(), Some(head));

    // A restart restores that state instead of opening an empty room.
    let restored = Room::load_or_create(&cellar, room_id, 42, 8).await.unwrap();
    assert_eq!(restored.head_tin, Some(head));
    assert!(restored.scene_biblio.contains_key("tavern"));
}

// spec: alhalte-banquette/persistence 0011 a broken head alias fails startup
#[tokio::test]
async fn test_load_or_create_fails_on_a_broken_head_alias() {
    let cellar = InMemoryCellarSink::new();
    let room_id = 2011u64;
    let alias = room_head_alias(room_id);
    let assert_restore_error = |err: PersistenceError| {
        assert!(
            err.to_string().contains("failed to restore room"),
            "a restore failure must be explicit, got: {err}"
        );
    };

    // 1. The alias points at a tin that is not stored.
    cellar.set_alias(&alias, &[0xABu8; 32]).await.unwrap();
    let err = Room::load_or_create(&cellar, room_id, 42, 8)
        .await
        .expect_err("a missing tin must fail startup");
    assert_restore_error(err);

    // 2. The alias points at a stored payload that is not a RoomData tin.
    let (wrong_label_bytes, wrong_label_id) = Tin::from_string("not room data")
        .encode_plaintext(tin::Compression::Zstd)
        .unwrap();
    cellar
        .store_tin(42, 0, &wrong_label_bytes, &wrong_label_id, None)
        .await
        .unwrap();
    cellar.set_alias(&alias, &wrong_label_id).await.unwrap();
    let err = Room::load_or_create(&cellar, room_id, 42, 8)
        .await
        .expect_err("a wrong label must fail startup");
    assert_restore_error(err);

    // 3. The stored bytes do not hash to the aliased tin id.
    let (_, expected_id) = Tin::from_string("hash mismatch expected")
        .encode_plaintext(tin::Compression::Zstd)
        .unwrap();
    let (other_bytes, _) = Tin::from_string("hash mismatch actual")
        .encode_plaintext(tin::Compression::Zstd)
        .unwrap();
    cellar
        .store_tin(42, 0, &other_bytes, &expected_id, None)
        .await
        .unwrap();
    cellar.set_alias(&alias, &expected_id).await.unwrap();
    let err = Room::load_or_create(&cellar, room_id, 42, 8)
        .await
        .expect_err("a content hash mismatch must fail startup");
    assert_restore_error(err);

    // 4. A valid RoomData for another room ID is not opened as this room.
    let mut other = Room::new(room_id + 1, 42, 8).unwrap();
    let other_head = other.persist(&cellar).await.unwrap();
    cellar.set_alias(&alias, &other_head).await.unwrap();
    let err = Room::load_or_create(&cellar, room_id, 42, 8)
        .await
        .expect_err("a room ID mismatch must fail startup");
    assert_restore_error(err);

    // None of the failures replaced the alias, so the broken head is still the
    // recorded restore point until an operator fixes cellar.
    assert_eq!(cellar.get_alias(&alias).await.unwrap(), Some(other_head));
}

// spec: alhalte-banquette/persistence 0008 the cellar service token is never logged
#[test]
fn test_http_cellar_sink_redacts_service_token() {
    let sink = alhalte_banquette::HttpCellarSink::new("http://127.0.0.1:1")
        .with_service_token("internal-service-token");
    assert!(sink.has_service_token());
    let debug = format!("{sink:?}");
    assert!(
        !debug.contains("internal-service-token"),
        "service token leaked in Debug: {debug}"
    );
    assert!(debug.contains("<redacted>"));

    let untokened = alhalte_banquette::HttpCellarSink::new("http://127.0.0.1:1");
    assert!(!untokened.has_service_token());
}

// spec: alhalte-banquette/persistence 0009 SIGTERM runs the graceful final persist
#[tokio::test]
async fn test_sigterm_runs_graceful_final_persist() {
    use alhalte_cellar::{CellarService, InMemorySessionVerifier};

    // A real cellar HTTP service using the shared internal token.
    let storage = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    let service = Arc::new(
        CellarService::new_with_in_memory_db(storage.path(), verifier, false)
            .unwrap()
            .with_service_token("sigterm-service-token"),
    );
    let app = Arc::clone(&service).into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cellar_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("banquette.sock");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_alhalte-banquette"))
        .arg("--room-id")
        .arg("5151515151515151")
        .arg("--admin")
        .arg("10")
        .arg("--socket")
        .arg(&sock_path)
        .arg("--cellar-url")
        .arg(format!("http://{cellar_addr}"))
        .arg("--service-token")
        .arg("sigterm-service-token")
        .arg("--heartbeat-interval-secs")
        .arg("3600")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn the banquette binary");

    for _ in 0..100 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sock_path.exists(), "banquette did not bind its UNIX socket");

    let stream = UnixStream::connect(&sock_path).await.unwrap();
    let (read_half, write_half) = stream.into_split();
    let mut reader = Framed::new(read_half, ProtocolCodec::<BanquetteToMaitreD>::new());
    let mut writer = Framed::new(write_half, ProtocolCodec::<MaitreDToBanquette>::new());
    writer.send(MaitreDToBanquette::Ping).await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), reader.next()).await {
            Ok(Some(Ok(BanquetteToMaitreD::Pong))) => break,
            Ok(Some(Ok(_))) => continue,
            other => panic!("expected Pong from the standalone binary: {other:?}"),
        }
    }
    // Give the spawned signal task a chance to install its handlers.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pid = child.id().expect("child pid");
    // SAFETY: `kill(2)` is an FFI call to the OS and `pid` is a live child of
    // this test; no pointer or Rust memory is involved.
    let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0, "failed to send SIGTERM to banquette");

    // The final persist is observable as the moved `room/<room_id>/head` alias.
    let room_id = 0x5151515151515151u64;
    let alias = room_head_alias(room_id);
    let mut head = None;
    for _ in 0..40 {
        if let Some(tin_id) = service.get_alias(&alias).await.unwrap() {
            head = Some(tin_id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let tin_id = head.expect("SIGTERM must move the final room head alias");
    assert!(
        service.download(&tin_id, None).await.is_ok(),
        "the final RoomData must be stored in cellar"
    );

    let sink = alhalte_banquette::HttpCellarSink::new(format!("http://{cellar_addr}"))
        .with_service_token("sigterm-service-token");
    let restored = Room::load_from_cellar(&sink, &tin_id, room_id)
        .await
        .expect("the final room must restore through the service token");
    assert_eq!(
        restored.default_permission,
        RoomPermissionLevel::Denied,
        "a newly created room must stay closed"
    );

    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("banquette must exit after SIGTERM")
        .unwrap();
    assert!(
        status.success(),
        "banquette must exit cleanly, got {status:?}"
    );
}

// spec: alhalte-maitre-d/debug-login 0002 --no-persist keeps the room out of cellar
#[tokio::test]
async fn test_no_persist_never_writes_cellar() {
    use alhalte_cellar::{CellarService, InMemorySessionVerifier};

    // A real cellar HTTP service: anything uploaded would be visible here.
    let storage = tempdir().unwrap();
    let verifier = Arc::new(InMemorySessionVerifier::new());
    let service =
        Arc::new(CellarService::new_with_in_memory_db(storage.path(), verifier, false).unwrap());
    let app = Arc::clone(&service).into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cellar_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("banquette.sock");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_alhalte-banquette"))
        .args(["--room-id", "6161616161616161", "--admin", "10"])
        .arg("--socket")
        .arg(&sock_path)
        .arg("--cellar-url")
        .arg(format!("http://{cellar_addr}"))
        .args(["--heartbeat-interval-secs", "3600", "--no-persist"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn the banquette binary");

    for _ in 0..100 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sock_path.exists(), "banquette did not bind its UNIX socket");

    let stream = UnixStream::connect(&sock_path).await.unwrap();
    let (read_half, write_half) = stream.into_split();
    let mut reader = Framed::new(read_half, ProtocolCodec::<BanquetteToMaitreD>::new());
    let mut writer = Framed::new(write_half, ProtocolCodec::<MaitreDToBanquette>::new());
    // The room works without any cellar round trip.
    writer.send(MaitreDToBanquette::Ping).await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), reader.next()).await {
            Ok(Some(Ok(BanquetteToMaitreD::Pong))) => break,
            Ok(Some(Ok(_))) => continue,
            other => panic!("expected Pong from the no-persist binary: {other:?}"),
        }
    }
    // Give the spawned signal task a chance to install its handlers.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pid = child.id().expect("child pid");
    // SAFETY: `kill(2)` is an FFI call to the OS and `pid` is a live child of
    // this test; no pointer or Rust memory is involved.
    let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0, "failed to send SIGTERM to banquette");

    // No head is reported in no-persist mode: the room lives only in the child's
    // in-memory sink, so there is no tin to download from the shared cellar.

    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("banquette must exit after SIGTERM")
        .unwrap();
    assert!(
        status.success(),
        "banquette must exit cleanly, got {status:?}"
    );
}
