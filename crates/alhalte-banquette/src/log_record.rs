//! Duck-typed inspection of client-supplied log entries (Issue #271).
//!
//! A log entry is opaque to banquette: the room process stores the `Tin` it is
//! handed and never decodes the payload, so the `content` / `speaker` /
//! `speakerId` key convention belongs to cutlery (see the "既知の差分・制約"
//! section of the crate's `ARCHITECTURE.md`).
//!
//! That duck typing leaves one field with security meaning: an entry may claim
//! another account's character as its speaker. [`entry_speaker_id`] extracts that
//! claim without adopting a dependency on the whole record schema, so
//! [`crate::room::Room::append_log`] / [`crate::room::Room::edit_log`] can
//! authorize it. Entries that are not JSON, or that carry no `speakerId`, are
//! narration and stay opaque.

use serde_json::Value;
use tin::Tin;

use crate::error::{BanquetteError, Result};

/// Key a log record uses to name the character it speaks as.
const SPEAKER_ID_KEY: &str = "speakerId";

/// Returns the character id a client-supplied log entry claims to speak as.
///
/// The payload is interpreted as JSON only when it parses as a JSON object, so
/// the behavior is:
///
/// - payload is not JSON, or not a JSON object → `Ok(None)`;
/// - `speakerId` is absent or `null` → `Ok(None)` (narration);
/// - `speakerId` is a non-negative JSON integer, or a decimal string that fits
///   in a `u64` → `Ok(Some(character_id))`;
/// - anything else (`1.5`, `-1`, `true`, `{}`, `"abc"`, an id above `u64::MAX`)
///   → `Err(BanquetteError::Serialization)`.
///
/// A malformed `speakerId` is an error instead of being folded into narration:
/// silently downgrading it would let a client append a record whose displayed
/// speaker differs from the one the authorization check inspected.
pub fn entry_speaker_id(entry: &Tin) -> Result<Option<u64>> {
    let Ok(value) = serde_json::from_slice::<Value>(&entry.payload) else {
        return Ok(None);
    };
    let Some(object) = value.as_object() else {
        return Ok(None);
    };
    let Some(raw) = object.get(SPEAKER_ID_KEY) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }

    let parsed = if let Some(number) = raw.as_u64() {
        Some(number)
    } else {
        raw.as_str().and_then(|text| text.parse::<u64>().ok())
    };

    parsed.map(Some).ok_or_else(|| {
        BanquetteError::Serialization(format!("invalid {SPEAKER_ID_KEY} in log entry: {raw}"))
    })
}
