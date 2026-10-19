//! Specification tests for the four-level room permission model (Issue #199).
//!
//! Each test references the corresponding section of `ARCHITECTURE.md`.
//!
//! The model under test:
//!
//! - `Denied`: no read and no write anywhere.
//! - `Viewer`: reads every scene, writes nothing.
//! - `Participant`: reads and writes `"/"` plus the scenes an administrator
//!   moved it into.
//! - `Admin`: reads and writes every scene and changes the permission table.

use std::collections::HashMap;

use alhalte_banquette::Character;
use alhalte_banquette::cellar_sink::{CellarSink, InMemoryCellarSink};
use alhalte_banquette::character::CharacterBiblio;
use alhalte_banquette::error::BanquetteError;
use alhalte_banquette::labels::LABEL_ALHALTE_BANQUETTE;
use alhalte_banquette::room::{Room, RoomData, RoomPermissionLevel};
use alhalte_banquette::scene::SceneData;
use serde::Serialize;
use tin::Tin;
use tin::TinId;

/// Builds a room exercising every permission level.
///
/// - account 1 is the creator and stays `Admin`.
/// - account 2 is a `Viewer`.
/// - account 3 is a `Participant` with `"alpha"` granted through a character
///   move, and owns character id 1.
/// - account 4 is a second `Admin`.
/// - account 5 has no entry, so the closed default permission makes it `Denied`.
fn permission_fixture() -> (Room, u64) {
    let mut room = Room::new(7001, 1, 8).unwrap();
    room.create_scene(1, "alpha", None).unwrap();
    room.create_scene(1, "beta", None).unwrap();
    room.set_member_permission(1, 2, RoomPermissionLevel::Viewer)
        .unwrap();
    room.set_member_permission(1, 3, RoomPermissionLevel::Participant)
        .unwrap();
    room.set_member_permission(1, 4, RoomPermissionLevel::Admin)
        .unwrap();

    // A participant only receives a scene through an administrator move.
    let mut data = Character::new();
    alhalte_banquette::set_character_name(&mut data, "P3");
    let character_id = room.create_character(3, data).unwrap();
    room.move_character(1, character_id, "alpha").unwrap();

    room.validate().expect("the fixture is a valid room");
    (room, character_id)
}

#[test]
// spec: alhalte-banquette/permissions 0001 four levels x scene read/write matrix
fn test_permission_read_write_matrix() {
    let (room, _) = permission_fixture();
    let scenes = ["/", "alpha", "beta"];

    // (account, expected level, readable scenes, writable scenes)
    let expectations: [(u64, RoomPermissionLevel, [bool; 3], [bool; 3]); 4] = [
        (
            5,
            RoomPermissionLevel::Denied,
            [false, false, false],
            [false, false, false],
        ),
        (
            2,
            RoomPermissionLevel::Viewer,
            [true, true, true],
            [false, false, false],
        ),
        (
            3,
            RoomPermissionLevel::Participant,
            [true, true, false],
            [true, true, false],
        ),
        (
            4,
            RoomPermissionLevel::Admin,
            [true, true, true],
            [true, true, true],
        ),
    ];

    for (account, level, readable, writable) in expectations {
        assert_eq!(
            room.get_account_permission(account),
            level,
            "account {account} must resolve to {level:?}"
        );
        for (index, scene) in scenes.iter().enumerate() {
            assert_eq!(
                room.can_access_scene(account, scene),
                readable[index],
                "{level:?} read access to {scene}"
            );
            assert_eq!(
                room.can_send_to_scene(account, scene),
                writable[index],
                "{level:?} write access to {scene}"
            );
        }
    }

    // `can_write` is the coarse predicate behind the mutate checks.
    assert!(!RoomPermissionLevel::Denied.can_write());
    assert!(!RoomPermissionLevel::Viewer.can_write());
    assert!(RoomPermissionLevel::Participant.can_write());
    assert!(RoomPermissionLevel::Admin.can_write());

    // A scene that does not exist is never accessible.
    assert!(!room.can_access_scene(4, "ghost"));
    assert!(!room.can_send_to_scene(4, "ghost"));
}

#[test]
// spec: alhalte-banquette/permissions 0004 at least one administrator, last demotion rejected
fn test_last_admin_demotion_is_rejected() {
    let mut room = Room::new(7002, 1, 8).unwrap();
    assert_eq!(room.admin_accounts(), vec![1]);

    // The only administrator cannot leave the admin level.
    assert!(matches!(
        room.set_member_permission(1, 1, RoomPermissionLevel::Participant),
        Err(BanquetteError::LastAdminDemotion)
    ));
    assert!(matches!(
        room.set_member_permission(1, 1, RoomPermissionLevel::Viewer),
        Err(BanquetteError::LastAdminDemotion)
    ));
    assert!(matches!(
        room.set_member_permission(1, 1, RoomPermissionLevel::Denied),
        Err(BanquetteError::LastAdminDemotion)
    ));
    assert_eq!(room.get_account_permission(1), RoomPermissionLevel::Admin);

    // With a second administrator the demotion is allowed.
    room.set_member_permission(1, 2, RoomPermissionLevel::Admin)
        .unwrap();
    room.set_member_permission(1, 1, RoomPermissionLevel::Participant)
        .unwrap();
    assert_eq!(room.admin_accounts(), vec![2]);
    room.validate().expect("one administrator remains");

    // A persisted room without any administrator is rejected, never reopened as
    // an empty room.
    let mut data: RoomData = Room::new(7003, 1, 8)
        .unwrap()
        .to_tin()
        .unwrap()
        .to_postcard()
        .unwrap();
    data.permissions.clear();
    data.default_permission = RoomPermissionLevel::Denied;
    let tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &data).unwrap();
    assert!(matches!(
        Room::from_tin(&tin),
        Err(BanquetteError::InvalidRoomData(_))
    ));
}

#[test]
// spec: alhalte-banquette/permissions 0005 accessible_scenes reference existing scenes only
fn test_accessible_scenes_invariant() {
    let (mut room, character_id) = permission_fixture();
    assert_eq!(
        room.accessible_scenes_of(3),
        ["/".to_string(), "alpha".to_string()]
    );

    // A viewer never carries scene grants: demoting drops them.
    room.set_member_permission(1, 3, RoomPermissionLevel::Viewer)
        .unwrap();
    assert!(room.accessible_scenes_of(3).is_empty());
    room.validate().unwrap();

    // Promoting back re-adds root access; the administrator grants the scene
    // again by moving the character into it.
    room.set_member_permission(1, 3, RoomPermissionLevel::Participant)
        .unwrap();
    room.move_character(1, character_id, "alpha").unwrap();
    assert_eq!(
        room.accessible_scenes_of(3),
        ["/".to_string(), "alpha".to_string()]
    );

    // Deleting a scene drops the grants that referenced it.
    room.delete_scene(1, "alpha").unwrap();
    assert_eq!(room.accessible_scenes_of(3), ["/".to_string()]);
    room.validate().unwrap();

    // A persisted room referencing a missing scene is invalid.
    let mut data: RoomData = room.to_tin().unwrap().to_postcard().unwrap();
    data.accessible_scenes.insert(3, vec!["ghost".to_string()]);
    let tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &data).unwrap();
    assert!(matches!(
        Room::from_tin(&tin),
        Err(BanquetteError::InvalidRoomData(_))
    ));

    // A persisted room granting scenes to a viewer is invalid.
    let mut data: RoomData = room.to_tin().unwrap().to_postcard().unwrap();
    data.permissions.insert(2, RoomPermissionLevel::Viewer);
    data.accessible_scenes.insert(2, vec!["/".to_string()]);
    let tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &data).unwrap();
    assert!(matches!(
        Room::from_tin(&tin),
        Err(BanquetteError::InvalidRoomData(_))
    ));
}

#[tokio::test]
// spec: alhalte-banquette/persistence 0001 the permission table survives persist and restore
async fn test_permission_table_survives_persist_and_restore() {
    let cellar = InMemoryCellarSink::new();
    let (mut room, _) = permission_fixture();
    room.set_default_permission(1, RoomPermissionLevel::Viewer)
        .unwrap();

    let head = room.persist(&cellar).await.unwrap();
    let restored = Room::load_from_cellar(&cellar, &head, room.room_id)
        .await
        .unwrap();

    assert_eq!(restored.room_id, room.room_id);
    assert_eq!(restored.default_permission, room.default_permission);
    assert_eq!(restored.permissions, room.permissions);
    assert_eq!(restored.accessible_scenes, room.accessible_scenes);
    assert_eq!(restored.chunk_exponent, room.chunk_exponent);
    assert_eq!(restored.list_scene_ids(), room.list_scene_ids());
    restored.validate().expect("a restored room is valid");
    assert_eq!(
        restored.get_account_permission(2),
        RoomPermissionLevel::Viewer
    );
    assert_eq!(
        restored.get_account_permission(3),
        RoomPermissionLevel::Participant
    );
}

/// `RoomData` as persisted before Issue #199 (the removed `done` flag schema).
#[derive(Serialize)]
struct LegacyRoomData {
    room_id: u64,
    done: bool,
    chunk_exponent: u8,
    character_biblio: CharacterBiblio,
    scenes: HashMap<String, SceneData>,
}

#[tokio::test]
// spec: alhalte-banquette/persistence 0002 the pre-#199 RoomData schema fails loudly
async fn test_legacy_room_data_is_rejected_not_empty() {
    // Reuse a current payload so the legacy scenes are well-formed; only the
    // room-level schema differs.
    let current: RoomData = Room::new(7004, 1, 8)
        .unwrap()
        .to_tin()
        .unwrap()
        .to_postcard()
        .unwrap();
    let legacy = LegacyRoomData {
        room_id: current.room_id,
        done: true,
        chunk_exponent: current.chunk_exponent,
        character_biblio: current.character_biblio.clone(),
        scenes: current.scenes.clone(),
    };
    let tin = Tin::from_postcard_label(LABEL_ALHALTE_BANQUETTE, &legacy).unwrap();

    let direct = Room::from_tin(&tin);
    assert!(
        direct.is_err(),
        "the pre-#199 schema must not decode into a room, got {direct:?}"
    );

    // The restore path fails as well instead of starting an empty room.
    let (bytes, tin_id): (Vec<u8>, TinId) = tin.encode_plaintext(tin::Compression::Zstd).unwrap();
    let cellar = InMemoryCellarSink::new();
    cellar.store_tin(1, 0, &bytes, &tin_id, None).await.unwrap();
    let restored = Room::load_from_cellar(&cellar, &tin_id, 7004).await;
    assert!(
        restored.is_err(),
        "the persisted pre-#199 schema must fail the spawn, got {restored:?}"
    );
}

#[tokio::test]
// spec: alhalte-banquette/permissions 0006 every mutate path is authorized in the room
async fn test_mutate_frames_are_authorized() {
    let cellar = InMemoryCellarSink::new();
    let (mut room, character_id) = permission_fixture();

    // Seed a log entry so edit / delete / rewind have a reachable target.
    room.append_log(1, 1, "alpha", Some(Tin::from_string("hello")), &cellar)
        .await
        .unwrap();

    // Viewer (2) is receive-only: every mutate is refused.
    assert!(matches!(
        room.append_log(2, 1, "alpha", Some(Tin::from_string("x")), &cellar)
            .await,
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.edit_log(2, 1, "alpha", 0, 0, Tin::from_string("x"), &cellar)
            .await,
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.delete_log(2, 1, "alpha", 0, 0, &cellar).await,
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.rewind_log(2, "alpha", 0, 0),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.update_scene_state(2, "alpha", "custom::k".to_string(), Tin::from_string("v")),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.delete_scene_state(2, "alpha", "custom::k"),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.create_scene(2, "gamma", None),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.delete_scene(2, "alpha"),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.create_character(2, Character::new()),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.update_character(2, character_id, Character::new()),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.delete_character(2, character_id),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(matches!(
        room.move_character(2, character_id, "beta"),
        Err(BanquetteError::PermissionDenied)
    ));

    // Participant (3) writes the granted scene and the root scene only.
    assert!(matches!(
        room.append_log(3, 1, "beta", Some(Tin::from_string("x")), &cellar)
            .await,
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(
        room.append_log(3, 1, "alpha", Some(Tin::from_string("x")), &cellar)
            .await
            .is_ok()
    );
    assert!(
        room.append_log(3, 1, "/", Some(Tin::from_string("x")), &cellar)
            .await
            .is_ok()
    );
    assert!(matches!(
        room.update_scene_state(3, "beta", "custom::k".to_string(), Tin::from_string("v")),
        Err(BanquetteError::PermissionDenied)
    ));
    assert!(
        room.update_scene_state(3, "alpha", "custom::k".to_string(), Tin::from_string("v"))
            .is_ok()
    );
    // Scene structure stays an administrator operation.
    assert!(matches!(
        room.create_scene(3, "gamma", None),
        Err(BanquetteError::PermissionDenied)
    ));
    // A participant cannot move even its own character: being moved is how an
    // administrator grants scene access.
    assert!(matches!(
        room.move_character(3, character_id, "beta"),
        Err(BanquetteError::PermissionDenied)
    ));
    // It can still edit its own character.
    assert!(
        room.update_character(3, character_id, Character::new())
            .is_ok()
    );

    // Admin (4) may do all of it.
    assert!(
        room.append_log(4, 1, "beta", Some(Tin::from_string("x")), &cellar)
            .await
            .is_ok()
    );
    assert!(
        room.edit_log(4, 1, "beta", 0, 0, Tin::from_string("y"), &cellar)
            .await
            .is_ok()
    );
    assert!(
        room.rewind_log(4, "beta", 0, 1).is_ok(),
        "admin may rewind the log head"
    );
    assert!(room.delete_scene_state(4, "beta", "custom::k").is_ok());
    assert!(room.create_scene(4, "gamma", None).is_ok());
    assert!(room.delete_scene(4, "gamma").is_ok());
    assert!(room.create_character(4, Character::new()).is_ok());
    assert!(room.move_character(4, character_id, "beta").is_ok());
    assert_eq!(
        room.accessible_scenes_of(3),
        ["/".to_string(), "alpha".to_string(), "beta".to_string()]
    );
}

#[tokio::test]
// spec: alhalte-banquette/permissions 0007 export follows can_access_scene
async fn test_export_filters_scenes_by_permission() {
    let cellar = InMemoryCellarSink::new();
    let (room, _) = permission_fixture();

    // A viewer sees every scene.
    let (viewer_data, _) = room.export_snapshot(&cellar, Some(2)).await.unwrap();
    for scene_id in ["/", "alpha", "beta"] {
        assert!(
            viewer_data.scenes.contains_key(scene_id),
            "a viewer must export {scene_id}"
        );
    }

    // A participant only sees its granted scenes plus the root scene.
    let (participant_data, _) = room.export_snapshot(&cellar, Some(3)).await.unwrap();
    assert!(participant_data.scenes.contains_key("/"));
    assert!(participant_data.scenes.contains_key("alpha"));
    assert!(!participant_data.scenes.contains_key("beta"));

    // A denied account exports no scene at all.
    let (denied_data, _) = room.export_snapshot(&cellar, Some(5)).await.unwrap();
    assert!(denied_data.scenes.is_empty());

    // The permission table itself is not filtered: it describes the room.
    assert_eq!(
        denied_data.permissions.get(&4),
        Some(&RoomPermissionLevel::Admin)
    );
}

#[test]
// spec: alhalte-banquette/permissions 0008 the removed done flag leaves no code behind
fn test_removed_done_symbols_are_absent() {
    // Free-form documentation may still explain the removed schema; the check is
    // on real code lines only.
    const FORBIDDEN: [&str; 6] = [
        "SetDone",
        "DoneUpdated",
        "AdminsUpdated",
        "std::admins",
        "KEY_STD_ADMINS",
        "set_done(",
    ];

    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut scanned = 0;
    for entry in std::fs::read_dir(&src).expect("src directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source file");
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for symbol in FORBIDDEN {
                assert!(
                    !line.contains(symbol),
                    "{}:{} still contains `{symbol}`",
                    path.display(),
                    index + 1
                );
            }
        }
        scanned += 1;
    }
    assert!(scanned > 0, "the scan must actually read source files");
}
