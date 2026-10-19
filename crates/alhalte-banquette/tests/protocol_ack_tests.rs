//! Protocol-level regression tests for the client <-> banquette JSON contract
//! and for the success acknowledgements added by Issue #136 (scene create/delete,
//! character move, and log append).
//!
//! Each test references the corresponding section of `ARCHITECTURE.md`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tempfile::tempdir;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;

use alhalte_banquette::cellar_sink::InMemoryCellarSink;
use alhalte_banquette::character::{Character, get_character_name, set_character_name};
use alhalte_banquette::protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
};
use alhalte_banquette::room::{Room, RoomPermissionLevel};
use alhalte_banquette::server::{BanquetteServer, ServerConfig};
use tin::Tin;

type ServerStream = Framed<OwnedReadHalf, ProtocolCodec<BanquetteToMaitreD>>;
type ClientSink = Framed<OwnedWriteHalf, ProtocolCodec<MaitreDToBanquette>>;

/// Builds a 64 hexadecimal character tin id whose 32 bytes are all `byte`.
fn tin_id_hex(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

#[test]
// spec: alhalte-banquette/protocol frame round-trips and variant indices
fn test_new_frames_are_appended_and_roundtrip() {
    let frames = [
        ServerFrame::SceneCreated {
            scene_id: "tavern".to_string(),
        },
        ServerFrame::SceneDeleted {
            scene_id: "tavern".to_string(),
        },
        ServerFrame::CharacterMoved {
            character_id: 7,
            to_scene_id: "dungeon".to_string(),
        },
        ServerFrame::PermissionsSnapshot {
            default_permission: RoomPermissionLevel::Participant,
            permissions: std::collections::BTreeMap::from([(10, RoomPermissionLevel::Admin)]),
        },
        ServerFrame::PermissionsUpdated {
            default_permission: RoomPermissionLevel::Viewer,
            permissions: std::collections::BTreeMap::from([(55, RoomPermissionLevel::Denied)]),
        },
    ];

    for frame in frames {
        let json = serde_json::to_string(&frame).unwrap();
        assert_eq!(serde_json::from_str::<ServerFrame>(&json).unwrap(), frame);

        let bytes = postcard::to_allocvec(&frame).unwrap();
        assert_eq!(postcard::from_bytes::<ServerFrame>(&bytes).unwrap(), frame);
    }

    // The permission frames round-trip on the client and control channels too.
    let client_frames = [
        ClientFrame::SetMemberPermission {
            account_id: 55,
            permission: RoomPermissionLevel::Admin,
        },
        ClientFrame::SetDefaultPermission {
            permission: RoomPermissionLevel::Viewer,
        },
        ClientFrame::QueryPermissions,
    ];
    for frame in client_frames {
        let json = serde_json::to_string(&frame).unwrap();
        assert_eq!(serde_json::from_str::<ClientFrame>(&json).unwrap(), frame);
        let bytes = postcard::to_allocvec(&frame).unwrap();
        assert_eq!(postcard::from_bytes::<ClientFrame>(&bytes).unwrap(), frame);
    }

    let query_account_permission = MaitreDToBanquette::QueryAccountPermission {
        request_id: 9,
        account_id: 55,
    };
    let bytes = postcard::to_allocvec(&query_account_permission).unwrap();
    assert_eq!(
        postcard::from_bytes::<MaitreDToBanquette>(&bytes).unwrap(),
        query_account_permission
    );
    let permission_response = BanquetteToMaitreD::AccountPermissionResponse {
        request_id: 9,
        account_id: 55,
        permission: RoomPermissionLevel::Viewer,
    };
    let bytes = postcard::to_allocvec(&permission_response).unwrap();
    assert_eq!(
        postcard::from_bytes::<BanquetteToMaitreD>(&bytes).unwrap(),
        permission_response
    );

    // The postcard variant index is a positional varint. Issue #199 removed
    // `SetDone` / `DoneUpdated` / `SetAdmins` / `AdminsUpdated`, so the indices
    // after the removed variant shifted; these values pin the new layout.
    // Issue #192 removed `RoomPersisted` from `BanquetteToMaitreD` (the head is
    // a cellar alias now), which shifts the control-channel indices below.
    assert_eq!(
        postcard::to_allocvec(&ClientFrame::HeartbeatResponse)
            .unwrap()
            .first(),
        Some(&22u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ClientFrame::SetMemberPermission {
            account_id: 0,
            permission: RoomPermissionLevel::Denied,
        })
        .unwrap()
        .first(),
        Some(&23u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ClientFrame::SetDefaultPermission {
            permission: RoomPermissionLevel::Denied,
        })
        .unwrap()
        .first(),
        Some(&24u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ClientFrame::QueryPermissions)
            .unwrap()
            .first(),
        Some(&25u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ServerFrame::Error {
            code: String::new(),
            message: String::new(),
        })
        .unwrap()
        .first(),
        Some(&18u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ServerFrame::SceneCreated {
            scene_id: String::new(),
        })
        .unwrap()
        .first(),
        Some(&19u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ServerFrame::CharacterMoved {
            character_id: 0,
            to_scene_id: String::new(),
        })
        .unwrap()
        .first(),
        Some(&21u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ServerFrame::PermissionsSnapshot {
            default_permission: RoomPermissionLevel::Denied,
            permissions: std::collections::BTreeMap::new(),
        })
        .unwrap()
        .first(),
        Some(&22u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ServerFrame::PermissionsUpdated {
            default_permission: RoomPermissionLevel::Denied,
            permissions: std::collections::BTreeMap::new(),
        })
        .unwrap()
        .first(),
        Some(&23u8)
    );
    assert_eq!(
        postcard::to_allocvec(&MaitreDToBanquette::QueryAccountPermission {
            request_id: 0,
            account_id: 0,
        })
        .unwrap()
        .first(),
        Some(&5u8)
    );
    assert_eq!(
        postcard::to_allocvec(&BanquetteToMaitreD::AccountPermissionResponse {
            request_id: 0,
            account_id: 0,
            permission: RoomPermissionLevel::Denied,
        })
        .unwrap()
        .first(),
        Some(&6u8)
    );
}

/// Starts a banquette server on a temporary UNIX socket and connects a fake maitre-d.
async fn start_test_server() -> (tempfile::TempDir, ClientSink, ServerStream, JoinHandle<()>) {
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("banquette.sock");

    let cellar = Arc::new(InMemoryCellarSink::new());
    let room = Room::new(3003, 10, 8).unwrap();
    let config = ServerConfig {
        socket_path: sock_path.clone(),
        heartbeat_interval: Duration::from_secs(3600),
    };

    let server = BanquetteServer::new(config, room, cellar);
    let server_task = tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Wait until the socket file exists.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client_stream = UnixStream::connect(&sock_path).await.unwrap();
    let (read_half, write_half) = client_stream.into_split();
    let stream = Framed::new(read_half, ProtocolCodec::<BanquetteToMaitreD>::new());
    let mut sink = Framed::new(write_half, ProtocolCodec::<MaitreDToBanquette>::new());

    // Client 100 is the room administrator (account 10).
    sink.send(MaitreDToBanquette::ClientConnected {
        client_id: 100,
        account_id: 10,
        session_id: 1,
    })
    .await
    .unwrap();

    (dir, sink, stream, server_task)
}

async fn send_frame(sink: &mut ClientSink, client_id: u64, frame: ClientFrame) {
    sink.send(MaitreDToBanquette::ClientMessage { client_id, frame })
        .await
        .unwrap();
}

async fn next_frame(stream: &mut ServerStream) -> BanquetteToMaitreD {
    match tokio::time::timeout(Duration::from_secs(2), stream.next()).await {
        Ok(Some(Ok(frame))) => frame,
        other => panic!("timed out waiting for a banquette frame: {other:?}"),
    }
}

async fn wait_for_frame(
    stream: &mut ServerStream,
    mut pred: impl FnMut(&BanquetteToMaitreD) -> bool,
) -> BanquetteToMaitreD {
    for _ in 0..16 {
        let frame = next_frame(stream).await;
        if pred(&frame) {
            return frame;
        }
    }
    panic!("expected frame was not emitted");
}

fn expect_send_to_client(frame: BanquetteToMaitreD) -> (u64, ServerFrame) {
    match frame {
        BanquetteToMaitreD::SendToClient { client_id, frame } => (client_id, frame),
        other => panic!("expected SendToClient, got {other:?}"),
    }
}

fn expect_broadcast_to_room(frame: BanquetteToMaitreD) -> ServerFrame {
    match frame {
        BanquetteToMaitreD::BroadcastToRoom { frame } => frame,
        other => panic!("expected BroadcastToRoom, got {other:?}"),
    }
}

fn expect_broadcast_to_scene(frame: BanquetteToMaitreD) -> (String, ServerFrame) {
    match frame {
        BanquetteToMaitreD::BroadcastToScene { scene_id, frame } => (scene_id, frame),
        other => panic!("expected BroadcastToScene, got {other:?}"),
    }
}

#[tokio::test]
// spec: alhalte-banquette/scene lifecycle success acks (Issue #136)
async fn test_scene_create_and_delete_acks() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // CreateScene -> SceneCreated to the requester and SceneList to the room.
    send_frame(
        &mut sink,
        100,
        ClientFrame::CreateScene {
            scene_id: "tavern".to_string(),
            inherit_juices: false,
            name: None,
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::SendToClient { .. })
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::SceneCreated { scene_id } => assert_eq!(scene_id, "tavern"),
        other => panic!("expected SceneCreated, got {other:?}"),
    }

    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::BroadcastToRoom { .. })
        })
        .await,
    ) {
        ServerFrame::SceneList { scenes } => assert!(scenes.iter().any(|s| s.id == "tavern")),
        other => panic!("expected SceneList, got {other:?}"),
    }

    // A non-admin client cannot delete a scene: DeleteSceneFailed is emitted.
    sink.send(MaitreDToBanquette::ClientConnected {
        client_id: 200,
        account_id: 20,
        session_id: 2,
    })
    .await
    .unwrap();
    send_frame(
        &mut sink,
        200,
        ClientFrame::DeleteScene {
            scene_id: "tavern".to_string(),
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::Error { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 200);
    match frame {
        ServerFrame::Error { code, .. } => assert_eq!(code, "DeleteSceneFailed"),
        other => panic!("expected Error, got {other:?}"),
    }

    // DeleteScene by the admin -> SceneDeleted plus a refreshed room scene list.
    send_frame(
        &mut sink,
        100,
        ClientFrame::DeleteScene {
            scene_id: "tavern".to_string(),
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::SceneDeleted { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::SceneDeleted { scene_id } => assert_eq!(scene_id, "tavern"),
        other => panic!("expected SceneDeleted, got {other:?}"),
    }

    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::BroadcastToRoom { .. })
        })
        .await,
    ) {
        ServerFrame::SceneList { scenes } => assert!(!scenes.iter().any(|s| s.id == "tavern")),
        other => panic!("expected SceneList, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
// spec: alhalte-banquette/character move success ack (Issue #136)
async fn test_character_move_ack() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    let mut data: Character = HashMap::new();
    set_character_name(&mut data, "Hero");
    send_frame(&mut sink, 100, ClientFrame::CreateCharacter { data }).await;

    let (_, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::CharacterCreated { .. },
                    ..
                }
            )
        })
        .await,
    );
    let character_id = match frame {
        ServerFrame::CharacterCreated { character_id } => character_id,
        other => panic!("expected CharacterCreated, got {other:?}"),
    };

    send_frame(
        &mut sink,
        100,
        ClientFrame::MoveCharacter {
            character_id,
            to_scene_id: "dungeon".to_string(),
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::CharacterMoved { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::CharacterMoved {
            character_id: moved_id,
            to_scene_id,
        } => {
            assert_eq!(moved_id, character_id);
            assert_eq!(to_scene_id, "dungeon");
        }
        other => panic!("expected CharacterMoved, got {other:?}"),
    }

    // `MoveCharacter` implicitly creates the destination scene, so the room's scene list
    // is re-broadcast (the room-wide `CharacterMoved` broadcast comes first).
    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::SceneList { .. }
                }
            )
        })
        .await,
    ) {
        ServerFrame::SceneList { scenes } => assert!(scenes.iter().any(|s| s.id == "dungeon")),
        other => panic!("expected SceneList, got {other:?}"),
    }

    // Moving a missing character reports MoveCharacterFailed.
    send_frame(
        &mut sink,
        100,
        ClientFrame::MoveCharacter {
            character_id: 9_999,
            to_scene_id: "dungeon".to_string(),
        },
    )
    .await;

    let (_, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::Error { .. },
                    ..
                }
            )
        })
        .await,
    );
    match frame {
        ServerFrame::Error { code, .. } => assert_eq!(code, "MoveCharacterFailed"),
        other => panic!("expected Error, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
// spec: alhalte-banquette/character room-wide CRUD + scene membership (Issue #160)
async fn test_character_crud_broadcasts_and_scene_membership() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // A second client joins the room: broadcasts are room-wide, not just an
    // echo to the requester.
    sink.send(MaitreDToBanquette::ClientConnected {
        client_id: 200,
        account_id: 20,
        session_id: 2,
    })
    .await
    .unwrap();

    // Create -> `CharacterCreated` ack (server id) + `CharacterInfo` / `SceneInfo`
    // broadcasts carrying the full attribute map and the new membership.
    let mut data: Character = HashMap::new();
    set_character_name(&mut data, "Hero");
    send_frame(&mut sink, 100, ClientFrame::CreateCharacter { data }).await;

    let (_, created) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::CharacterCreated { .. },
                    ..
                }
            )
        })
        .await,
    );
    let character_id = match created {
        ServerFrame::CharacterCreated { character_id } => character_id,
        other => panic!("expected CharacterCreated, got {other:?}"),
    };

    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::CharacterInfo { .. }
                }
            )
        })
        .await,
    ) {
        ServerFrame::CharacterInfo {
            character_id: id,
            data,
        } => {
            assert_eq!(id, character_id);
            assert_eq!(get_character_name(&data).as_deref(), Some("Hero"));
        }
        other => panic!("expected CharacterInfo, got {other:?}"),
    }

    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::SceneInfo(_)
                }
            )
        })
        .await,
    ) {
        ServerFrame::SceneInfo(summary) => {
            assert_eq!(summary.id, "/");
            assert_eq!(summary.present_character_count, 1);
            assert_eq!(summary.present_characters, vec![character_id]);
        }
        other => panic!("expected SceneInfo, got {other:?}"),
    }

    // Update -> `CharacterUpdated` ack + a `CharacterInfo` broadcast with the
    // stored attributes (so the character tab and the field agree).
    let mut updated: Character = HashMap::new();
    set_character_name(&mut updated, "Hero II");
    send_frame(
        &mut sink,
        100,
        ClientFrame::UpdateCharacter {
            character_id,
            data: updated,
        },
    )
    .await;
    wait_for_frame(&mut stream, |f| {
        matches!(
            f,
            BanquetteToMaitreD::SendToClient {
                frame: ServerFrame::CharacterUpdated { .. },
                ..
            }
        )
    })
    .await;
    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::CharacterInfo { .. }
                }
            )
        })
        .await,
    ) {
        ServerFrame::CharacterInfo {
            character_id: id,
            data,
        } => {
            assert_eq!(id, character_id);
            assert_eq!(get_character_name(&data).as_deref(), Some("Hero II"));
        }
        other => panic!("expected CharacterInfo, got {other:?}"),
    }

    // Store a piece placement so deletion can prove the cleanup path.
    let placed_key = format!("piece::{character_id}::placed");
    send_frame(
        &mut sink,
        100,
        ClientFrame::UpdateState {
            scene_id: "/".to_string(),
            key: placed_key.clone(),
            value: Tin::from_bool(true),
        },
    )
    .await;
    wait_for_frame(&mut stream, |f| {
        matches!(
            f,
            BanquetteToMaitreD::SendToClient {
                frame: ServerFrame::StateUpdated { .. },
                ..
            }
        )
    })
    .await;

    // Move -> direct ack + room-wide `CharacterMoved`, and a summary for both the
    // source and the destination scene.
    send_frame(
        &mut sink,
        100,
        ClientFrame::MoveCharacter {
            character_id,
            to_scene_id: "tavern".to_string(),
        },
    )
    .await;
    wait_for_frame(&mut stream, |f| {
        matches!(
            f,
            BanquetteToMaitreD::SendToClient {
                frame: ServerFrame::CharacterMoved { .. },
                ..
            }
        )
    })
    .await;
    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::CharacterMoved { .. }
                }
            )
        })
        .await,
    ) {
        ServerFrame::CharacterMoved {
            character_id: id,
            to_scene_id,
        } => {
            assert_eq!(id, character_id);
            assert_eq!(to_scene_id, "tavern");
        }
        other => panic!("expected CharacterMoved, got {other:?}"),
    }

    let mut root_membership = None;
    let mut tavern_membership = None;
    for _ in 0..12 {
        let frame = next_frame(&mut stream).await;
        if let BanquetteToMaitreD::BroadcastToRoom {
            frame: ServerFrame::SceneInfo(summary),
        } = frame
        {
            match summary.id.as_str() {
                "/" => root_membership = Some(summary.present_characters),
                "tavern" => tavern_membership = Some(summary.present_characters),
                _ => {}
            }
        }
        if root_membership.is_some() && tavern_membership.is_some() {
            break;
        }
    }
    assert_eq!(root_membership, Some(Vec::new()));
    assert_eq!(tavern_membership, Some(vec![character_id]));

    // Delete -> direct ack + room-wide `CharacterDeleted`, the stale piece state
    // is deleted, and the membership summary drops the character.
    send_frame(
        &mut sink,
        100,
        ClientFrame::DeleteCharacter { character_id },
    )
    .await;
    wait_for_frame(&mut stream, |f| {
        matches!(
            f,
            BanquetteToMaitreD::SendToClient {
                frame: ServerFrame::CharacterDeleted { .. },
                ..
            }
        )
    })
    .await;
    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::CharacterDeleted { .. }
                }
            )
        })
        .await,
    ) {
        ServerFrame::CharacterDeleted { character_id: id } => assert_eq!(id, character_id),
        other => panic!("expected CharacterDeleted, got {other:?}"),
    }

    wait_for_frame(&mut stream, |f| match f {
        BanquetteToMaitreD::SendToClient {
            frame: ServerFrame::StateDeleted { key, .. },
            ..
        }
        | BanquetteToMaitreD::BroadcastToScene {
            frame: ServerFrame::StateDeleted { key, .. },
            ..
        } => key == &placed_key,
        _ => false,
    })
    .await;

    let mut tavern_empty = false;
    for _ in 0..8 {
        let frame = next_frame(&mut stream).await;
        if let BanquetteToMaitreD::BroadcastToRoom {
            frame: ServerFrame::SceneInfo(summary),
        } = frame
            && summary.id == "tavern"
        {
            tavern_empty =
                summary.present_characters.is_empty() && summary.present_character_count == 0;
            break;
        }
    }
    assert!(
        tavern_empty,
        "deletion must remove the character from the destination scene"
    );

    server_task.abort();
}

#[tokio::test]
// spec: alhalte-banquette/log append position and ack (Issue #136)
async fn test_append_log_ack() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // A JSON payload carrying `clientOpId` must survive the roundtrip unchanged.
    let log_payload = br#"{"content":"hello","speaker":"Hero","clientOpId":"op-1"}"#.to_vec();
    let log_tin = Tin::need_not_know(log_payload.clone());
    send_frame(
        &mut sink,
        100,
        ClientFrame::AppendLog {
            scene_id: "/".to_string(),
            entry: Some(log_tin.clone()),
        },
    )
    .await;

    let (scene_id, frame) = expect_broadcast_to_scene(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToScene {
                    frame: ServerFrame::LogEventAppended { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(scene_id, "/");
    match frame {
        ServerFrame::LogEventAppended {
            scene_id,
            chunk_index,
            position,
            entry,
        } => {
            assert_eq!(scene_id, "/");
            assert_eq!((chunk_index, position), (0, 0));
            assert_eq!(entry, Some(log_tin));
        }
        other => panic!("expected LogEventAppended, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
// spec: alhalte-banquette/state success ack for requesters outside the target scene (Issue #136)
async fn test_state_acks_are_direct_and_broadcast() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // Client 100 is placed in "/" on connect. Create a different scene so that the
    // requester is not in the scene whose state it edits (room assets / extensions).
    send_frame(
        &mut sink,
        100,
        ClientFrame::CreateScene {
            scene_id: "tavern".to_string(),
            inherit_juices: false,
            name: None,
        },
    )
    .await;
    wait_for_frame(&mut stream, |f| {
        matches!(
            f,
            BanquetteToMaitreD::SendToClient {
                frame: ServerFrame::SceneCreated { .. },
                ..
            }
        )
    })
    .await;
    wait_for_frame(&mut stream, |f| {
        matches!(f, BanquetteToMaitreD::BroadcastToRoom { .. })
    })
    .await;

    // UpdateState: the requester always receives a direct StateUpdated ack...
    send_frame(
        &mut sink,
        100,
        ClientFrame::UpdateState {
            scene_id: "tavern".to_string(),
            key: "atmos::fog".to_string(),
            value: Tin::from_string("thick"),
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::StateUpdated { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::StateUpdated {
            scene_id,
            key,
            value,
        } => {
            assert_eq!(scene_id, "tavern");
            assert_eq!(key, "atmos::fog");
            assert_eq!(value.as_string().unwrap(), "thick");
        }
        other => panic!("expected StateUpdated, got {other:?}"),
    }

    // ...and the scene broadcast still goes out for the other clients in the scene.
    let (scene_id, frame) = expect_broadcast_to_scene(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToScene {
                    frame: ServerFrame::StateUpdated { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(scene_id, "tavern");
    match frame {
        ServerFrame::StateUpdated { key, .. } => assert_eq!(key, "atmos::fog"),
        other => panic!("expected StateUpdated, got {other:?}"),
    }

    // A key without `::` is rejected without a broadcast.
    send_frame(
        &mut sink,
        100,
        ClientFrame::UpdateState {
            scene_id: "tavern".to_string(),
            key: "invalid".to_string(),
            value: Tin::from_string("x"),
        },
    )
    .await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::Error { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::Error { code, .. } => assert_eq!(code, "UpdateStateFailed"),
        other => panic!("expected Error, got {other:?}"),
    }

    // DeleteState: direct ack plus scene broadcast.
    send_frame(
        &mut sink,
        100,
        ClientFrame::DeleteState {
            scene_id: "tavern".to_string(),
            key: "atmos::fog".to_string(),
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::StateDeleted { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::StateDeleted { scene_id, key } => {
            assert_eq!(scene_id, "tavern");
            assert_eq!(key, "atmos::fog");
        }
        other => panic!("expected StateDeleted, got {other:?}"),
    }

    let (scene_id, frame) = expect_broadcast_to_scene(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::BroadcastToScene {
                    frame: ServerFrame::StateDeleted { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(scene_id, "tavern");
    match frame {
        ServerFrame::StateDeleted { key, .. } => assert_eq!(key, "atmos::fog"),
        other => panic!("expected StateDeleted, got {other:?}"),
    }

    server_task.abort();
}

// spec: alhalte-banquette/permissions 0002 permission changes are admin-only and acknowledged
#[tokio::test]
async fn test_permission_frames_require_admin() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // Client 101 (account 55) is unknown to the permission table, so the closed
    // default permission makes it `Denied`.
    sink.send(MaitreDToBanquette::ClientConnected {
        client_id: 101,
        account_id: 55,
        session_id: 2,
    })
    .await
    .unwrap();

    // A non-admin SetMemberPermission is rejected with
    // SetMemberPermissionFailed and never emits PermissionsUpdated.
    send_frame(
        &mut sink,
        101,
        ClientFrame::SetMemberPermission {
            account_id: 55,
            permission: RoomPermissionLevel::Admin,
        },
    )
    .await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::SendToClient { .. })
        })
        .await,
    );
    assert_eq!(client_id, 101);
    assert!(
        matches!(&frame, ServerFrame::Error { code, .. } if code == "SetMemberPermissionFailed"),
        "expected SetMemberPermissionFailed, got {frame:?}"
    );

    // A non-admin SetDefaultPermission is rejected too.
    send_frame(
        &mut sink,
        101,
        ClientFrame::SetDefaultPermission {
            permission: RoomPermissionLevel::Viewer,
        },
    )
    .await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::SendToClient { .. })
        })
        .await,
    );
    assert_eq!(client_id, 101);
    assert!(
        matches!(&frame, ServerFrame::Error { code, .. } if code == "SetDefaultPermissionFailed"),
        "expected SetDefaultPermissionFailed, got {frame:?}"
    );

    // The last administrator cannot demote itself (Issues #199).
    send_frame(
        &mut sink,
        100,
        ClientFrame::SetMemberPermission {
            account_id: 10,
            permission: RoomPermissionLevel::Participant,
        },
    )
    .await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::Error { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    assert!(
        matches!(frame, ServerFrame::Error { code, .. } if code == "SetMemberPermissionFailed")
    );

    // The room administrator promotes account 55: no restart needed.
    send_frame(
        &mut sink,
        100,
        ClientFrame::SetMemberPermission {
            account_id: 55,
            permission: RoomPermissionLevel::Admin,
        },
    )
    .await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::PermissionsUpdated { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::PermissionsUpdated { permissions, .. } => {
            assert_eq!(permissions.get(&55), Some(&RoomPermissionLevel::Admin))
        }
        other => panic!("expected PermissionsUpdated, got {other:?}"),
    }

    // The promoted account now succeeds and the ack carries the new table.
    send_frame(
        &mut sink,
        101,
        ClientFrame::SetDefaultPermission {
            permission: RoomPermissionLevel::Viewer,
        },
    )
    .await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::PermissionsUpdated { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 101);
    match frame {
        ServerFrame::PermissionsUpdated {
            default_permission, ..
        } => assert_eq!(default_permission, RoomPermissionLevel::Viewer),
        other => panic!("expected PermissionsUpdated, got {other:?}"),
    }

    // QueryPermissions answers an account with access with the full table.
    send_frame(&mut sink, 100, ClientFrame::QueryPermissions).await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::PermissionsSnapshot { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::PermissionsSnapshot {
            default_permission,
            permissions,
        } => {
            assert_eq!(default_permission, RoomPermissionLevel::Viewer);
            assert_eq!(permissions.get(&10), Some(&RoomPermissionLevel::Admin));
            assert_eq!(permissions.get(&55), Some(&RoomPermissionLevel::Admin));
        }
        other => panic!("expected PermissionsSnapshot, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
// spec: alhalte-banquette/scene state hydration on EnterScene (Issue #159)
async fn test_enter_scene_sends_state_snapshot() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // Populate the root scene state with the keys the client hydrates.
    let states = [
        ("std::name", Tin::from_string("広間")),
        ("std::custom", Tin::from_string(&tin_id_hex(0x11))),
        ("std::custom4", Tin::from_string(&tin_id_hex(0x44))),
        ("std::asset", Tin::from_string(r#"{"tinId":"deadbeef"}"#)),
        ("std::asset::width", Tin::from_u64(1280)),
        ("std::asset::height", Tin::from_u64(720)),
        (
            "view::transform",
            Tin::from_string(r#"{"x":1,"y":2,"scale":1.5}"#),
        ),
        ("piece::7::placed", Tin::from_u64(1)),
    ];
    for (key, value) in states {
        send_frame(
            &mut sink,
            100,
            ClientFrame::UpdateState {
                scene_id: "/".to_string(),
                key: key.to_string(),
                value,
            },
        )
        .await;
        wait_for_frame(&mut stream, |frame| {
            matches!(
                frame,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::StateUpdated { .. },
                    ..
                }
            )
        })
        .await;
    }

    // Entering the scene must answer the caller with a StateSnapshot without a
    // separate QueryState request.
    send_frame(
        &mut sink,
        100,
        ClientFrame::EnterScene {
            scene_id: "/".to_string(),
        },
    )
    .await;

    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |frame| {
            matches!(
                frame,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::StateSnapshot { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    match frame {
        ServerFrame::StateSnapshot { scene_id, states } => {
            assert_eq!(scene_id, "/");
            assert_eq!(
                states.get("std::name").and_then(|tin| tin.as_string().ok()),
                Some("広間")
            );
            assert_eq!(
                states
                    .get("std::custom4")
                    .and_then(|tin| tin.as_string().ok()),
                Some(tin_id_hex(0x44).as_str())
            );
            assert!(states.contains_key("std::asset"));
            assert!(states.contains_key("std::asset::width"));
            assert!(states.contains_key("std::asset::height"));
            assert!(states.contains_key("view::transform"));
            assert!(states.contains_key("piece::7::placed"));
        }
        other => panic!("expected StateSnapshot, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
// spec: alhalte-banquette/scene name create / rename broadcast (Issue #159)
async fn test_scene_name_is_summarized_and_broadcast() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // The root scene has a default server-authoritative name.
    send_frame(
        &mut sink,
        100,
        ClientFrame::QueryScene {
            scene_id: "/".to_string(),
        },
    )
    .await;
    let (_, root_info) = expect_send_to_client(
        wait_for_frame(&mut stream, |frame| {
            matches!(
                frame,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::SceneInfo(_),
                    ..
                }
            )
        })
        .await,
    );
    match root_info {
        ServerFrame::SceneInfo(summary) => {
            assert_eq!(summary.id, "/");
            assert_eq!(summary.name, "メインシーン");
            assert_eq!(summary.present_character_count, 0);
        }
        other => panic!("expected SceneInfo, got {other:?}"),
    }

    // CreateScene carries the name atomically, so SceneList already has it.
    send_frame(
        &mut sink,
        100,
        ClientFrame::CreateScene {
            scene_id: "tavern".to_string(),
            inherit_juices: false,
            name: Some("酒場".to_string()),
        },
    )
    .await;
    wait_for_frame(&mut stream, |frame| {
        matches!(
            frame,
            BanquetteToMaitreD::SendToClient {
                frame: ServerFrame::SceneCreated { .. },
                ..
            }
        )
    })
    .await;

    let list = expect_broadcast_to_room(
        wait_for_frame(&mut stream, |frame| {
            matches!(
                frame,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::SceneList { .. }
                }
            )
        })
        .await,
    );
    match list {
        ServerFrame::SceneList { scenes } => {
            let tavern = scenes
                .iter()
                .find(|summary| summary.id == "tavern")
                .expect("tavern must be listed");
            assert_eq!(tavern.name, "酒場");
        }
        other => panic!("expected SceneList, got {other:?}"),
    }

    // Create also announces the new summary so a client can add it immediately.
    let created_info = expect_broadcast_to_room(
        wait_for_frame(&mut stream, |frame| {
            matches!(
                frame,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::SceneInfo(_)
                }
            )
        })
        .await,
    );
    match created_info {
        ServerFrame::SceneInfo(summary) => {
            assert_eq!(summary.id, "tavern");
            assert_eq!(summary.name, "酒場");
        }
        other => panic!("expected SceneInfo, got {other:?}"),
    }

    // Renaming goes through UpdateState and re-broadcasts SceneInfo.
    send_frame(
        &mut sink,
        100,
        ClientFrame::UpdateState {
            scene_id: "tavern".to_string(),
            key: "std::name".to_string(),
            value: Tin::from_string("大酒場"),
        },
    )
    .await;
    let renamed_info = expect_broadcast_to_room(
        wait_for_frame(&mut stream, |frame| {
            matches!(
                frame,
                BanquetteToMaitreD::BroadcastToRoom {
                    frame: ServerFrame::SceneInfo(_)
                }
            )
        })
        .await,
    );
    match renamed_info {
        ServerFrame::SceneInfo(summary) => {
            assert_eq!(summary.id, "tavern");
            assert_eq!(summary.name, "大酒場");
        }
        other => panic!("expected SceneInfo, got {other:?}"),
    }

    server_task.abort();
}
