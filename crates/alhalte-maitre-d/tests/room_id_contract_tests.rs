//! Cross-language room ID contract tests.
//!
//! The same fixture is consumed by `web/alhalte-argenterie/tests/room_socket.test.ts`
//! so the Rust parser and the TypeScript WebSocket URL builder cannot drift.

use std::path::PathBuf;

use alhalte_maitre_d::RoomId;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RoomIds {
    generated_hex: Vec<String>,
    legacy_decimal: Vec<String>,
}

fn fixture() -> RoomIds {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("contracts")
        .join("room_ids.json");
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&json).expect("contracts/room_ids.json must be valid")
}

// spec: contracts/room_ids.json room ID contract (Rust side)
#[test]
fn canonical_hex_ids_are_accepted_by_room_id_parser() {
    for value in fixture().generated_hex {
        let parsed: RoomId = value
            .parse()
            .unwrap_or_else(|error| panic!("{value} should parse: {error}"));
        assert_eq!(parsed.to_string(), value);
        assert_eq!(RoomId::from(parsed.as_u64()), parsed);
    }
}

#[test]
fn legacy_decimal_ids_are_accepted_and_normalized_to_hex() {
    for value in fixture().legacy_decimal {
        let parsed: RoomId = value
            .parse()
            .unwrap_or_else(|error| panic!("{value} should parse: {error}"));
        let canonical = parsed.to_string();
        assert_eq!(canonical.len(), 16);
        assert!(canonical.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(canonical.parse::<RoomId>().unwrap(), parsed);
    }
}

#[test]
fn invalid_room_ids_are_rejected_by_room_id_parser() {
    for value in ["room-1", "not-a-room", "0x1234", "gggggggggggggggg"] {
        assert!(value.parse::<RoomId>().is_err(), "{value} must be rejected");
    }
}
