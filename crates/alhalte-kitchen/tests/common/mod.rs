//! Shared test helpers for alhalte-kitchen integration tests.

use std::collections::{BTreeMap, HashMap};

use alhalte_banquette::character::{CharacterBiblio, set_character_name};
use alhalte_banquette::log::{CharacterCameEvent, CharacterGoneEvent, LogBiblio, LogChunk};
use alhalte_banquette::room::{RoomData, RoomPermissionLevel};
use alhalte_banquette::scene::SceneData;
use alhalte_saignant::LogEntry;
use generast::Generast;
use tin::{Compression, Tin, TinId};

/// Helper to wrap a [`LogEntry`] into a [`Tin`].
pub fn log_to_tin(entry: LogEntry) -> Tin {
    let map = entry.to_map();
    let bytes = postcard::to_allocvec(&map).unwrap();
    Tin::new(0, bytes)
}

/// Helper to create a multi-scene test room with Alice and Bob transitioning between scenes.
pub fn create_test_room_data() -> (RoomData, Vec<(TinId, LogChunk)>) {
    let mut character_biblio = CharacterBiblio::new();
    let mut alice_char = HashMap::new();
    set_character_name(&mut alice_char, "Alice");
    let alice_id = character_biblio.create_character(1001, alice_char);

    let mut bob_char = HashMap::new();
    set_character_name(&mut bob_char, "Bob");
    let bob_id = character_biblio.create_character(1001, bob_char);

    // Scenes: "prologue", "forest", "castle"
    let mut chunks = Vec::new();

    // 1. Prologue chunk
    let prologue_entries = vec![
        Some(log_to_tin(
            LogEntry::new()
                .with_speaker_id(alice_id)
                .with_content_ast(&Generast::string("Let us venture into the forest!"))
                .with_unix_time(100),
        )),
        Some(log_to_tin(
            LogEntry::new()
                .with_speaker_id(bob_id)
                .with_content_ast(&Generast::string("I have prepared the torches."))
                .with_unix_time(120),
        )),
        // Alice leaves prologue
        Some(
            CharacterGoneEvent {
                character_id: alice_id,
            }
            .to_tin()
            .unwrap(),
        ),
    ];
    let chunk_prologue = LogChunk::new(prologue_entries);
    let chunk_prologue_tin = chunk_prologue.to_tin().unwrap();
    let (_, tid_prologue) = chunk_prologue_tin
        .encode_plaintext(Compression::Zstd)
        .unwrap();
    chunks.push((tid_prologue, chunk_prologue));

    // 2. Forest chunk
    let forest_entries = vec![
        // Alice enters forest
        Some(
            CharacterCameEvent {
                character_id: alice_id,
            }
            .to_tin()
            .unwrap(),
        ),
        Some(log_to_tin(
            LogEntry::new()
                .with_speaker_id(alice_id)
                .with_content_ast(&Generast::string("The trees are so tall!"))
                .with_unix_time(200),
        )),
        // Alice leaves forest
        Some(
            CharacterGoneEvent {
                character_id: alice_id,
            }
            .to_tin()
            .unwrap(),
        ),
    ];
    let chunk_forest = LogChunk::new(forest_entries);
    let chunk_forest_tin = chunk_forest.to_tin().unwrap();
    let (_, tid_forest) = chunk_forest_tin
        .encode_plaintext(Compression::Zstd)
        .unwrap();
    chunks.push((tid_forest, chunk_forest));

    // 3. Castle chunk
    let castle_entries = vec![
        // Alice enters castle
        Some(
            CharacterCameEvent {
                character_id: alice_id,
            }
            .to_tin()
            .unwrap(),
        ),
        Some(log_to_tin(
            LogEntry::new()
                .with_speaker_id(alice_id)
                .with_content_ast(&Generast::string("We have reached the castle gates."))
                .with_unix_time(300),
        )),
    ];
    let chunk_castle = LogChunk::new(castle_entries);
    let chunk_castle_tin = chunk_castle.to_tin().unwrap();
    let (_, tid_castle) = chunk_castle_tin
        .encode_plaintext(Compression::Zstd)
        .unwrap();
    chunks.push((tid_castle, chunk_castle));

    let mut scenes = HashMap::new();
    scenes.insert(
        "prologue".to_string(),
        SceneData {
            id: "prologue".to_string(),
            states: BTreeMap::new(),
            biblio: LogBiblio {
                chunks: vec![tid_prologue],
                head: (1, 0),
            },
            present_characters: vec![alice_id, bob_id],
        },
    );
    scenes.insert(
        "forest".to_string(),
        SceneData {
            id: "forest".to_string(),
            states: BTreeMap::new(),
            biblio: LogBiblio {
                chunks: vec![tid_forest],
                head: (1, 0),
            },
            present_characters: vec![],
        },
    );
    scenes.insert(
        "castle".to_string(),
        SceneData {
            id: "castle".to_string(),
            states: BTreeMap::new(),
            biblio: LogBiblio {
                chunks: vec![tid_castle],
                head: (1, 0),
            },
            present_characters: vec![],
        },
    );

    let room_data = RoomData {
        room_id: 42,
        default_permission: RoomPermissionLevel::Viewer,
        permissions: BTreeMap::new(),
        accessible_scenes: BTreeMap::new(),
        chunk_exponent: 8,
        character_biblio,
        scenes,
    };

    (room_data, chunks)
}
