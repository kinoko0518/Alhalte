//! Data models and protocol types for `alhalte-cellar`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tin::TinId;

use crate::error::CellarError;

/// Helper to parse a 256-bit tin ID from a 64-character hex string.
pub fn parse_tin_id(s: &str) -> Result<TinId, CellarError> {
    let clean = s.trim();
    if clean.len() != 64 {
        return Err(CellarError::Internal(format!(
            "Invalid tin ID length: expected 64 hex characters, got {}",
            clean.len()
        )));
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(clean, &mut out)?;
    Ok(out)
}

/// Helper to format a 256-bit tin ID as a lowercase 64-character hex string.
pub fn format_tin_id(tin_id: &TinId) -> String {
    hex::encode(tin_id)
}

/// Maximum accepted length of a generic alias name, in bytes.
///
/// The longest name the ecosystem uses is `room/<16 hex>/head`, so the limit is
/// generous while still bounding what a client can store in the primary key.
pub const MAX_ALIAS_NAME_BYTES: usize = 256;

/// Validates a generic alias name such as `room/<room_id>/head`.
///
/// An alias name is an opaque key, not a filesystem path, so `/` is allowed and
/// hierarchical names are the intended use. Names must be 1..=
/// [`MAX_ALIAS_NAME_BYTES`] bytes and consist only of ASCII letters, digits and
/// `-`, `_`, `.`, `:`, `/`. Control characters, whitespace and non-ASCII bytes
/// are rejected so a name can always be embedded in a URL path and a log line.
///
/// # Errors
///
/// Returns [`CellarError::InvalidAlias`] for an empty, over-long, or
/// out-of-charset name. The message never echoes a rejected name.
pub fn validate_alias_name(name: &str) -> Result<&str, CellarError> {
    let allowed =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/');
    if name.is_empty() {
        return Err(CellarError::InvalidAlias(
            "alias names must not be empty".to_string(),
        ));
    }
    if name.len() > MAX_ALIAS_NAME_BYTES {
        return Err(CellarError::InvalidAlias(format!(
            "alias names must be at most {MAX_ALIAS_NAME_BYTES} bytes"
        )));
    }
    if let Some(offset) = name.bytes().position(|byte| !allowed(byte)) {
        return Err(CellarError::InvalidAlias(format!(
            "alias names allow only ASCII alphanumerics and -_.:/ (offset {offset})"
        )));
    }
    Ok(name)
}

/// Request body of `PUT /alias/{name}`.
///
/// The name travels in the URL path, so the body only carries the target tin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasRequest {
    /// 256-bit tin ID the alias must point at.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
}

/// Response body of `GET /alias/{name}` / `PUT /alias/{name}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AliasResponse {
    /// 256-bit tin ID the alias points at.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
}

/// Serde module to serialize/deserialize `TinId` as a hex string.
pub mod hex_tin_id {
    use super::*;

    /// Serializes `TinId` as a hex string.
    pub fn serialize<S>(tin_id: &TinId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format_tin_id(tin_id))
    }

    /// Deserializes `TinId` from a hex string or byte slice.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<TinId, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TinIdVisitor;

        impl<'de> serde::de::Visitor<'de> for TinIdVisitor {
            type Value = TinId;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter
                    .write_str("a 64-character hex string or 32-byte array representing a tin ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<TinId, E>
            where
                E: serde::de::Error,
            {
                parse_tin_id(value).map_err(serde::de::Error::custom)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<TinId, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut bytes = [0u8; 32];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(bytes)
            }
        }

        deserializer.deserialize_any(TinIdVisitor)
    }
}

/// Serde module for optional byte buffers serialized as hex strings.
pub mod opt_hex_bytes {
    use super::*;

    /// Serializes `Option<Vec<u8>>` as an optional hex string.
    pub fn serialize<S>(opt: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt {
            Some(bytes) => serializer.serialize_some(&hex::encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes `Option<Vec<u8>>` from an optional hex string.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => {
                if s.is_empty() {
                    Ok(None)
                } else {
                    hex::decode(s).map(Some).map_err(serde::de::Error::custom)
                }
            }
            None => Ok(None),
        }
    }
}

/// Reference label used for writes authorized by the internal service token.
///
/// `owner` is **not** an authorization boundary: sessions and the internal
/// service token authorize a write, while `owner` only labels the reference row
/// for attribution, refcounting, and GC. Service-token writes therefore use
/// this constant instead of a caller-supplied account ID, so no request body can
/// choose the label of an internal write. `0` is safe as a sentinel because
/// account IDs are issued starting from `1`.
pub const SYSTEM_OWNER: u64 = 0;

/// Represents a stored reference to a tin entity in the SQLite database.
///
/// In accordance with ARCHITECTURE.md:
/// `(tin_id: [u8; 32], owner: u64, wrapped_key: Option<Vec<u8>>)`
///
/// The entity file itself is located by `hex(tin_id)`, so no path is stored.
/// `owner` is a reference label (see [`SYSTEM_OWNER`]), never an authorization
/// boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TinReference {
    /// 256-bit Tin ID.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
    /// Reference label: the owner account ID for user writes, or
    /// [`SYSTEM_OWNER`] for service-token writes.
    pub owner: u64,
    /// Nullable wrapped key.
    #[serde(default, with = "opt_hex_bytes")]
    pub wrapped_key: Option<Vec<u8>>,
}

/// Store metadata request received from a client.
///
/// `owner` and `session_id` authenticate user writes. For service-token writes
/// they are ignored: the reference label is always [`SYSTEM_OWNER`]. The fields
/// stay in the type until the separate cleanup that removes them (#191).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreRequest {
    /// Owner account ID (reference label for user writes; ignored with a service token).
    pub owner: u64,
    /// Active session ID issued by `maitre-d` (ignored with a service token).
    pub session_id: u64,
    /// 256-bit tin ID.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
    /// Size of the tin binary in bytes.
    pub size: u64,
    /// Optional wrapped key (e.g., convergent encryption key encrypted by room key).
    #[serde(default, with = "opt_hex_bytes")]
    pub wrapped_key: Option<Vec<u8>>,
}

/// Result of the initial store metadata handshake (`POST /store`).
///
/// Serialized as a snake_case string by [`StoreResponse`]. The handshake
/// always answers `200 OK`; only the body distinguishes these outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreStatus {
    /// Client must upload the tin body data.
    UploadRequired,
    /// Entity already stored under another reference label; deduplicated and DB reference added.
    Deduplicated,
    /// A reference with the same label already exists.
    AlreadyOwned,
}

/// JSON body returned by the store metadata endpoint (`POST /store`).
///
/// The endpoint always answers `200 OK`; `result` carries the handshake
/// outcome, e.g. `{"result":"upload_required"}`. This is unrelated to
/// [`StoreResult`], which is the payload upload response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreResponse {
    /// Store check outcome.
    pub result: StoreStatus,
}

/// Result of storing a tin payload after upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreResult {
    /// 256-bit tin ID.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
    /// Reference label actually stored: the session owner for user writes, or
    /// [`SYSTEM_OWNER`] for service-token writes.
    pub owner: u64,
    /// Whether the file was written to disk (`true`), or the upload was discarded (`false`).
    pub written_to_disk: bool,
    /// Whether a new DB row was registered.
    pub reference_added: bool,
}

/// Unregister request to remove ownership of a tin.
///
/// `owner` and `session_id` authenticate user writes. For service-token writes
/// they are ignored and the [`SYSTEM_OWNER`] reference is removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnregisterRequest {
    /// Owner account ID (reference label for user writes; ignored with a service token).
    pub owner: u64,
    /// Active session ID (ignored with a service token).
    pub session_id: u64,
    /// 256-bit tin ID.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
}

/// Statistics reported by the Garbage Collector (GC).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcStats {
    /// Number of files scanned in the storage directory.
    pub scanned_files: usize,
    /// Number of unreferenced entity files deleted.
    pub deleted_files: usize,
    /// Total bytes freed from disk.
    pub freed_bytes: u64,
}
