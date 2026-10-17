//! Tests for query serialization modes (character path and scene ID list).

mod common;

use common::create_test_room_data;

use alhalte_kitchen::{KitchenError, RoomArchive, RoomGraph, export_room_to_zip};

#[test]
// spec: alhalte-kitchen/query character path start to end
fn test_character_path_query() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).unwrap();
    let graph = RoomGraph::build(&archive).unwrap();

    let session = graph
        .serialize_by_character_path(1, "prologue", "castle")
        .expect("Serialization of Alice's path should succeed");

    // Path must follow prologue -> forest -> castle (3 scenes)
    assert_eq!(session.scenes.len(), 3);
    assert_eq!(session.scenes[0].scene_id, "prologue");
    assert_eq!(session.scenes[1].scene_id, "forest");
    assert_eq!(session.scenes[2].scene_id, "castle");

    // Total logs
    assert_eq!(session.total_log_count(), 4); // 2 in prologue, 1 in forest, 1 in castle
    assert_eq!(
        session.character_names.get(&1).map(|s| s.as_str()),
        Some("Alice")
    );
}

#[test]
// spec: alhalte-kitchen/query scene id list
fn test_scene_id_list_query() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).unwrap();
    let graph = RoomGraph::build(&archive).unwrap();

    // Query scenes in reverse order: ["castle", "prologue"]
    let session = graph
        .serialize_by_scene_ids(&["castle", "prologue"])
        .expect("Serialization by scene ID list should succeed");

    assert_eq!(session.scenes.len(), 2);
    assert_eq!(session.scenes[0].scene_id, "castle");
    assert_eq!(session.scenes[1].scene_id, "prologue");

    assert_eq!(session.scenes[0].logs.len(), 1);
    assert_eq!(session.scenes[1].logs.len(), 2);
}

#[test]
// spec: alhalte-kitchen/query path not found error
fn test_path_not_found_error() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).unwrap();
    let graph = RoomGraph::build(&archive).unwrap();

    // Bob (id = 2) never moved to castle
    let result = graph.serialize_by_character_path(2, "prologue", "castle");
    assert!(
        matches!(result, Err(KitchenError::PathNotFound { .. })),
        "Tracing path for Bob to castle should return PathNotFound"
    );
}

#[test]
// spec: alhalte-kitchen/query scene not found error
fn test_scene_not_found_error() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).unwrap();
    let graph = RoomGraph::build(&archive).unwrap();

    let result = graph.serialize_by_scene_ids(&["nonexistent_scene"]);
    assert!(
        matches!(result, Err(KitchenError::SceneNotFound(_))),
        "Querying nonexistent scene should return SceneNotFound"
    );
}
