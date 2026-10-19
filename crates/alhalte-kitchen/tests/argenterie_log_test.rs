//! Tests for decoding the shared `alhalte/argenterie/log-record` schema.

use std::collections::{BTreeMap, HashMap};

use alhalte_banquette::character::{CharacterBiblio, set_character_name};
use alhalte_banquette::log::{LogBiblio, LogChunk};
use alhalte_banquette::room::{RoomData, RoomPermissionLevel};
use alhalte_banquette::scene::SceneData;
use alhalte_kitchen::{RoomArchive, RoomGraph, SceneEvent, export_room_to_zip};
use generast::Generast;
use tin::{Compression, Tin};

const ARGENTERIE_LOG_RECORD: &str = "alhalte/argenterie/log-record";

/// Wraps an old `HashMap<String, Tin>` log entry into a label-0 Tin.
fn log_to_tin(entry: alhalte_saignant::LogEntry) -> Tin {
    Tin::new(
        0,
        postcard::to_allocvec(&entry.to_map()).expect("serialize postcard map"),
    )
}

/// Wraps an `alhalte/argenterie/log-record` JSON payload into a Tin.
fn argenterie_log_tin(payload: &serde_json::Value) -> Tin {
    Tin::new(
        tin::label_hash(ARGENTERIE_LOG_RECORD),
        serde_json::to_vec(payload).expect("serialize log record"),
    )
}

/// Builds a single-scene room whose chunk holds the supplied entries.
fn room_with_entries(entries: Vec<Option<Tin>>) -> (RoomData, Vec<(tin::TinId, LogChunk)>) {
    let mut character_biblio = CharacterBiblio::new();
    let mut alice = HashMap::new();
    set_character_name(&mut alice, "Alice");
    let alice_id = character_biblio.create_character(1001, alice);

    let chunk = LogChunk::new(entries);
    let (_, chunk_tid) = chunk
        .to_tin()
        .expect("chunk tin")
        .encode_plaintext(Compression::Zstd)
        .expect("encode chunk");

    let mut scenes = HashMap::new();
    scenes.insert(
        "main".to_string(),
        SceneData {
            id: "main".to_string(),
            states: BTreeMap::new(),
            biblio: LogBiblio {
                chunks: vec![chunk_tid],
                head: (1, 0),
            },
            present_characters: vec![alice_id],
        },
    );

    let room = RoomData {
        room_id: 161,
        default_permission: RoomPermissionLevel::Viewer,
        permissions: BTreeMap::new(),
        accessible_scenes: BTreeMap::new(),
        chunk_exponent: 8,
        character_biblio,
        scenes,
    };
    (room, vec![(chunk_tid, chunk)])
}

// spec: alhalte-kitchen/archive argenterie log-record (JSON + contentTin)
#[test]
fn test_argenterie_log_record_survives_export_to_html() {
    const MESSAGE: &str = "拡張機能を通した実際のメッセージ";
    const TIMESTAMP: u64 = 1_700_000_000; // 2023-11-14T22:13:20Z

    let content_tin = generast::encode_ext_io(&Generast::string(MESSAGE), 1)
        .expect("ext-io tin")
        .to_plaintext();
    let record = serde_json::json!({
        "id": "log-argenterie-1",
        "sceneId": "main",
        "content": MESSAGE,
        "speaker": "Alice",
        "speakerId": "1",
        "timestamp": TIMESTAMP,
        "contentTin": content_tin,
    });

    let (room, chunks) = room_with_entries(vec![Some(argenterie_log_tin(&record))]);
    let zip_bytes = export_room_to_zip(&room, &chunks, None).expect("zip export failed");
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).expect("zip load failed");
    let graph = RoomGraph::build(&archive).expect("graph build failed");
    let session = graph
        .serialize_by_scene_ids(&["main"])
        .expect("serialize scene");

    // The contentTin is decoded as (Generast, speaker_id), and the speakerId is
    // resolved to the room character name.
    assert_eq!(session.total_log_count(), 1);
    assert_eq!(
        session.scenes[0].logs[0].content_ast(),
        Some(Generast::string(MESSAGE))
    );
    assert_eq!(session.scenes[0].logs[0].speaker_id(), Some(1));

    let html = session.to_html5();
    assert!(html.contains(MESSAGE), "HTML must contain the chat message");
    assert!(html.contains("Alice"), "HTML must contain the speaker name");
    assert!(
        html.contains("2023-11-14"),
        "HTML must contain the record timestamp"
    );
    assert!(!html.contains("Anonymous"), "speaker must not fall back");
    assert!(!html.contains("1970"), "timestamp must not fall back");
    assert!(!html.contains('\u{fffd}'), "HTML must not contain garbage");
}

// spec: alhalte-kitchen/archive argenterie log-record fallbacks
#[test]
fn test_argenterie_log_record_legacy_and_old_formats() {
    let mut character_biblio = CharacterBiblio::new();
    let mut alice = HashMap::new();
    set_character_name(&mut alice, "Alice");
    let alice_id = character_biblio.create_character(1001, alice);

    // New schema without `contentTin`: raw content is the fallback and the
    // name-only `speaker` is resolved through the room character biblio.
    let legacy = serde_json::json!({
        "id": "log-legacy",
        "sceneId": "main",
        "content": "legacy argenterie content",
        "speaker": "Alice",
        "timestamp": 100,
    });
    // Old postcard map of `HashMap<String, Tin>`.
    let postcard_map = log_to_tin(
        alhalte_saignant::LogEntry::new()
            .with_speaker_id(alice_id)
            .with_content_ast(&Generast::string("postcard map content"))
            .with_unix_time(110),
    );
    // Direct Generast Tin.
    let direct_generast = Generast::string("direct generast content")
        .to_tin()
        .expect("generast tin");
    // Primitive string Tin.
    let primitive = Tin::from_string("primitive string content");

    let chunk = LogChunk::new(vec![
        Some(argenterie_log_tin(&legacy)),
        Some(postcard_map),
        Some(direct_generast),
        Some(primitive),
    ]);
    let (_, chunk_tid) = chunk
        .to_tin()
        .expect("chunk tin")
        .encode_plaintext(Compression::Zstd)
        .expect("encode chunk");

    let mut scenes = HashMap::new();
    scenes.insert(
        "main".to_string(),
        SceneData {
            id: "main".to_string(),
            states: BTreeMap::new(),
            biblio: LogBiblio {
                chunks: vec![chunk_tid],
                head: (1, 0),
            },
            present_characters: vec![alice_id],
        },
    );
    let room = RoomData {
        room_id: 161,
        default_permission: RoomPermissionLevel::Viewer,
        permissions: BTreeMap::new(),
        accessible_scenes: BTreeMap::new(),
        chunk_exponent: 8,
        character_biblio,
        scenes,
    };

    let zip_bytes =
        export_room_to_zip(&room, &[(chunk_tid, chunk)], None).expect("zip export failed");
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).expect("zip load failed");
    let events = archive.scenes["main"].parse_all_events();
    let speeches: Vec<_> = events
        .into_iter()
        .filter_map(|event| match event.event {
            SceneEvent::Speech(entry) => Some(entry),
            _ => None,
        })
        .collect();

    assert_eq!(speeches.len(), 4);
    assert_eq!(
        speeches[0].content_ast(),
        Some(Generast::string("legacy argenterie content"))
    );
    assert_eq!(speeches[0].speaker_id(), Some(alice_id));
    assert_eq!(
        speeches[1].content_ast(),
        Some(Generast::string("postcard map content"))
    );
    assert_eq!(speeches[1].speaker_id(), Some(alice_id));
    assert_eq!(
        speeches[2].content_ast(),
        Some(Generast::string("direct generast content"))
    );
    assert_eq!(
        speeches[3].content_ast(),
        Some(Generast::string("primitive string content"))
    );
}
