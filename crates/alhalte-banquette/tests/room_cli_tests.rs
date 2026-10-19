//! CLI contract tests for room IDs.
//!
//! `contracts/room_ids.json` is shared with the maitre-d parser and the
//! argenterie WebSocket URL builder.

use std::path::PathBuf;

use alhalte_banquette::CliArgs;
use clap::Parser;
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

fn parse_room_id(value: &str) -> u64 {
    CliArgs::try_parse_from(["alhalte-banquette", "--room-id", value, "--admin", "1"])
        .unwrap_or_else(|error| panic!("--room-id {value} should parse: {error}"))
        .room_id
}

// spec: contracts/room_ids.json room ID contract (banquette CLI side)
#[test]
fn cli_accepts_canonical_hex_room_ids() {
    for value in fixture().generated_hex {
        let expected = u64::from_str_radix(&value, 16).unwrap();
        assert_eq!(parse_room_id(&value), expected);
    }
}

#[test]
fn cli_accepts_legacy_decimal_room_ids() {
    for value in fixture().legacy_decimal {
        let expected = value.parse::<u64>().unwrap();
        assert_eq!(parse_room_id(&value), expected);
    }
}

#[test]
fn cli_rejects_arbitrary_room_ids() {
    assert!(
        CliArgs::try_parse_from(["alhalte-banquette", "--room-id", "room-1", "--admin", "1",])
            .is_err()
    );
}
