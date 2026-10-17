//! Tests for archive loading (.zip and .tar.gz) and graph construction.

mod common;

use common::create_test_room_data;

use alhalte_kitchen::{NodeId, RoomArchive, RoomGraph, export_room_to_tar_gz, export_room_to_zip};

#[test]
// spec: alhalte-kitchen/archive zip
fn test_zip_archive_loading_and_graph_build() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).expect("zip export failed");

    let archive = RoomArchive::load_from_slice(&zip_bytes, None).expect("zip load failed");
    assert_eq!(archive.room_id, Some(42));
    assert_eq!(archive.scenes.len(), 3);
    assert_eq!(archive.characters.len(), 2);

    let graph = RoomGraph::build(&archive).expect("graph build failed");
    assert_eq!(graph.nodes.len(), 3);

    // Verify edges: Alice moves prologue -> forest -> castle
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.from.as_str().starts_with("prologue")
                && e.to.as_str().starts_with("forest")
                && e.character_id == 1),
        "Expected transition prologue -> forest for Alice"
    );
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.from.as_str().starts_with("forest")
                && e.to.as_str().starts_with("castle")
                && e.character_id == 1),
        "Expected transition forest -> castle for Alice"
    );
}

#[test]
// spec: alhalte-kitchen/archive tar.gz
fn test_tar_gz_archive_loading_and_graph_build() {
    let (room, chunks) = create_test_room_data();
    let tar_gz_bytes = export_room_to_tar_gz(&room, &chunks, None).expect("tar.gz export failed");

    let archive = RoomArchive::load_from_slice(&tar_gz_bytes, None).expect("tar.gz load failed");
    assert_eq!(archive.room_id, Some(42));
    assert_eq!(archive.scenes.len(), 3);
    assert_eq!(archive.characters.len(), 2);

    let graph = RoomGraph::build(&archive).expect("graph build failed");
    assert_eq!(graph.nodes.len(), 3);

    let path = graph
        .find_path_for_character(1, &NodeId::from("prologue"), &NodeId::from("castle"))
        .expect("Path for Alice should exist");
    assert_eq!(path.len(), 3);
}

#[test]
// spec: alhalte-kitchen/archive encrypted room key
fn test_room_key_encrypted_archive() {
    let (room, chunks) = create_test_room_data();
    let room_key = [0x42u8; 32];
    let zip_bytes =
        export_room_to_zip(&room, &chunks, Some(room_key)).expect("encrypted zip export failed");

    // Loading without key should fail
    let load_without_key = RoomArchive::load_from_slice(&zip_bytes, None);
    assert!(
        load_without_key.is_err(),
        "Loading encrypted archive without room key must fail"
    );

    // Loading with correct key should succeed
    let archive = RoomArchive::load_from_slice(&zip_bytes, Some(room_key))
        .expect("encrypted zip load failed");
    assert_eq!(archive.room_id, Some(42));
    assert_eq!(archive.scenes.len(), 3);
}
