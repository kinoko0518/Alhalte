//! Protocol-level regression tests for the client <-> banquette JSON contract
//! and for the success acknowledgements added by Issue #136 (scene create/delete,
//! character move, and BGM change logging).
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
use alhalte_banquette::character::{Character, set_character_name};
use alhalte_banquette::log::ChangeBgmEvent;
use alhalte_banquette::protocol::{
    BanquetteToMaitreD, ClientFrame, MaitreDToBanquette, ProtocolCodec, ServerFrame,
};
use alhalte_banquette::room::Room;
use alhalte_banquette::server::{BanquetteServer, ServerConfig};
use tin::Tin;

type ServerStream = Framed<OwnedReadHalf, ProtocolCodec<BanquetteToMaitreD>>;
type ClientSink = Framed<OwnedWriteHalf, ProtocolCodec<MaitreDToBanquette>>;

/// Builds a 64 hexadecimal character tin id whose 32 bytes are all `byte`.
fn tin_id_hex(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

#[test]
// spec: alhalte-banquette/protocol ChangeBgm frame JSON/postcard roundtrip
fn test_change_bgm_frame_serde_roundtrip() {
    let tin_id = tin_id_hex(0x07);
    let frame = ClientFrame::ChangeBgm {
        scene_id: "tavern".to_string(),
        slot: 2,
        tin_id: Some(tin_id.clone()),
    };

    // JSON (client -> maitre-d -> banquette) uses the externally tagged form.
    let json = serde_json::to_string(&frame).unwrap();
    assert_eq!(
        json,
        format!(r#"{{"ChangeBgm":{{"scene_id":"tavern","slot":2,"tin_id":"{tin_id}"}}}}"#)
    );
    assert_eq!(serde_json::from_str::<ClientFrame>(&json).unwrap(), frame);

    // `None` tin id encodes as JSON null and round-trips.
    let stop = ClientFrame::ChangeBgm {
        scene_id: "/".to_string(),
        slot: 1,
        tin_id: None,
    };
    let stop_json = serde_json::to_string(&stop).unwrap();
    assert_eq!(
        stop_json,
        r#"{"ChangeBgm":{"scene_id":"/","slot":1,"tin_id":null}}"#
    );
    assert_eq!(
        serde_json::from_str::<ClientFrame>(&stop_json).unwrap(),
        stop
    );

    // postcard (maitre-d <-> banquette) round-trips as well.
    let bytes = postcard::to_allocvec(&frame).unwrap();
    assert_eq!(postcard::from_bytes::<ClientFrame>(&bytes).unwrap(), frame);
}

#[test]
// spec: alhalte-banquette/protocol backward compatible variant indices
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
        ServerFrame::DoneUpdated { done: true },
    ];

    for frame in frames {
        let json = serde_json::to_string(&frame).unwrap();
        assert_eq!(serde_json::from_str::<ServerFrame>(&json).unwrap(), frame);

        let bytes = postcard::to_allocvec(&frame).unwrap();
        assert_eq!(postcard::from_bytes::<ServerFrame>(&bytes).unwrap(), frame);
    }

    // The permission-sync control frames round-trip as well.
    let set_admins = MaitreDToBanquette::SetAdmins {
        admins: vec![10, 55],
    };
    let bytes = postcard::to_allocvec(&set_admins).unwrap();
    assert_eq!(
        postcard::from_bytes::<MaitreDToBanquette>(&bytes).unwrap(),
        set_admins
    );
    let admins_updated = BanquetteToMaitreD::AdminsUpdated {
        admins: vec![10, 55],
    };
    let bytes = postcard::to_allocvec(&admins_updated).unwrap();
    assert_eq!(
        postcard::from_bytes::<BanquetteToMaitreD>(&bytes).unwrap(),
        admins_updated
    );

    // Backward compatibility: the postcard variant index is a positional varint, so new
    // variants must be appended last. These values pin the pre-existing indices.
    assert_eq!(
        postcard::to_allocvec(&ClientFrame::HeartbeatResponse)
            .unwrap()
            .first(),
        Some(&23u8)
    );
    assert_eq!(
        postcard::to_allocvec(&ClientFrame::ChangeBgm {
            scene_id: String::new(),
            slot: 1,
            tin_id: None,
        })
        .unwrap()
        .first(),
        Some(&24u8)
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
        postcard::to_allocvec(&ServerFrame::DoneUpdated { done: false })
            .unwrap()
            .first(),
        Some(&22u8)
    );
    assert_eq!(
        postcard::to_allocvec(&MaitreDToBanquette::SetAdmins { admins: vec![] })
            .unwrap()
            .first(),
        Some(&5u8)
    );
    assert_eq!(
        postcard::to_allocvec(&BanquetteToMaitreD::AdminsUpdated { admins: vec![] })
            .unwrap()
            .first(),
        Some(&7u8)
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
        admin_account: 10,
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
        ServerFrame::SceneList { scenes } => assert!(scenes.contains(&"tavern".to_string())),
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
        ServerFrame::SceneList { scenes } => assert!(!scenes.contains(&"tavern".to_string())),
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
    // is re-broadcast.
    match expect_broadcast_to_room(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::BroadcastToRoom { .. })
        })
        .await,
    ) {
        ServerFrame::SceneList { scenes } => assert!(scenes.contains(&"dungeon".to_string())),
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
// spec: alhalte-banquette/log append position and ChangeBgm ack (Issue #136)
async fn test_append_log_and_change_bgm_acks() {
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

    // ChangeBgm appends a ChangeBgmEvent and announces it with LogEventAppended.
    let hex_id = tin_id_hex(0x07);
    send_frame(
        &mut sink,
        100,
        ClientFrame::ChangeBgm {
            scene_id: "/".to_string(),
            slot: 2,
            tin_id: Some(hex_id),
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
            entry,
            chunk_index,
            position,
            ..
        } => {
            assert_eq!((chunk_index, position), (0, 1));
            let entry = entry.expect("ChangeBgm must append an entry");
            let event = ChangeBgmEvent::from_tin(&entry).unwrap();
            assert_eq!(event.slot, 2);
            assert_eq!(event.bgm_tin_id, Some([7u8; 32]));
            assert_eq!(event.volume, None);
            assert_eq!(event.track_index, None);
        }
        other => panic!("expected LogEventAppended, got {other:?}"),
    }

    // Stopping a slot uses `tin_id: None` and appends an event with no tin id.
    send_frame(
        &mut sink,
        100,
        ClientFrame::ChangeBgm {
            scene_id: "/".to_string(),
            slot: 2,
            tin_id: None,
        },
    )
    .await;

    let (_, frame) = expect_broadcast_to_scene(
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
    match frame {
        ServerFrame::LogEventAppended { entry, .. } => {
            let entry = entry.expect("ChangeBgm must append an entry");
            let event = ChangeBgmEvent::from_tin(&entry).unwrap();
            assert_eq!(event.slot, 2);
            assert_eq!(event.bgm_tin_id, None);
        }
        other => panic!("expected LogEventAppended, got {other:?}"),
    }

    // An invalid hex tin id is rejected without touching the log.
    send_frame(
        &mut sink,
        100,
        ClientFrame::ChangeBgm {
            scene_id: "/".to_string(),
            slot: 1,
            tin_id: Some("not-hex".to_string()),
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
        ServerFrame::Error { code, .. } => assert_eq!(code, "ChangeBgmFailed"),
        other => panic!("expected Error, got {other:?}"),
    }

    // A missing scene is reported as ChangeBgmFailed too.
    send_frame(
        &mut sink,
        100,
        ClientFrame::ChangeBgm {
            scene_id: "missing".to_string(),
            slot: 1,
            tin_id: None,
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
        ServerFrame::Error { code, .. } => assert_eq!(code, "ChangeBgmFailed"),
        other => panic!("expected Error, got {other:?}"),
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

// spec: alhalte-banquette/protocol SetDone success ack and admin-set control frame
#[tokio::test]
async fn test_set_done_ack_and_admin_set_control_frame() {
    let (_dir, mut sink, mut stream, server_task) = start_test_server().await;

    // Client 101 (account 55) is a plain participant.
    sink.send(MaitreDToBanquette::ClientConnected {
        client_id: 101,
        account_id: 55,
        session_id: 2,
    })
    .await
    .unwrap();

    // A non-admin SetDone is rejected and emits SetDoneFailed, never DoneUpdated.
    send_frame(&mut sink, 101, ClientFrame::SetDone { done: true }).await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(f, BanquetteToMaitreD::SendToClient { .. })
        })
        .await,
    );
    assert_eq!(client_id, 101);
    assert!(matches!(frame, ServerFrame::Error { code, .. } if code == "SetDoneFailed"));

    // maitre-d applies the database GM set without a room-client caller check.
    sink.send(MaitreDToBanquette::SetAdmins {
        admins: vec![10, 55],
    })
    .await
    .unwrap();
    match wait_for_frame(&mut stream, |f| {
        matches!(f, BanquetteToMaitreD::AdminsUpdated { .. })
    })
    .await
    {
        BanquetteToMaitreD::AdminsUpdated { admins } => assert_eq!(admins, vec![10, 55]),
        other => panic!("expected AdminsUpdated, got {other:?}"),
    }

    // The promoted account now succeeds; the success ack carries the new flag.
    send_frame(&mut sink, 101, ClientFrame::SetDone { done: true }).await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::DoneUpdated { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 101);
    assert_eq!(frame, ServerFrame::DoneUpdated { done: true });

    // The original administrator still succeeds too.
    send_frame(&mut sink, 100, ClientFrame::SetDone { done: false }).await;
    let (client_id, frame) = expect_send_to_client(
        wait_for_frame(&mut stream, |f| {
            matches!(
                f,
                BanquetteToMaitreD::SendToClient {
                    frame: ServerFrame::DoneUpdated { .. },
                    ..
                }
            )
        })
        .await,
    );
    assert_eq!(client_id, 100);
    assert_eq!(frame, ServerFrame::DoneUpdated { done: false });

    server_task.abort();
}
