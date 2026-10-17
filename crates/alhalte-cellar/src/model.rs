//! Data models and protocol types for `alhalte-cellar`.

use http::StatusCode;
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

/// Represents a stored reference to a tin entity in the SQLite database.
///
/// In accordance with ARCHITECTURE.md:
/// `(tin_id: [u8; 32], path: String, owner: u64, wrapped_key: Option<Vec<u8>>)`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TinReference {
    /// 256-bit Tin ID.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
    /// Path (or relative filename) pointing to the actual entity file in storage.
    pub path: String,
    /// Owner account ID.
    pub owner: u64,
    /// Nullable wrapped key.
    #[serde(default, with = "opt_hex_bytes")]
    pub wrapped_key: Option<Vec<u8>>,
}

/// Store metadata request received from a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreRequest {
    /// Owner account ID.
    pub owner: u64,
    /// Active session ID issued by `maitre-d`.
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

/// Response status returned from the initial store metadata handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StoreStatus {
    /// Status 210: Client must upload the tin body data.
    UploadRequired = 210,
    /// Status 211: Entity already stored by another owner; deduplicated and DB reference added.
    Deduplicated = 211,
    /// Status 212: Already owned by this owner.
    AlreadyOwned = 212,
}

impl StoreStatus {
    /// Converts this status to its numerical u16 HTTP status code.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Converts this status to an `http::StatusCode`.
    #[must_use]
    pub fn to_status_code(self) -> StatusCode {
        StatusCode::from_u16(self.as_u16()).unwrap_or(StatusCode::OK)
    }

    /// Human-readable explanation.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::UploadRequired => "Upload required (tin data must be transmitted)",
            Self::Deduplicated => "Deduplicated (reference registered without re-transmission)",
            Self::AlreadyOwned => "Already owned by requesting account",
        }
    }
}

/// JSON body returned from the store metadata endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreResponse {
    /// Numeric HTTP status code (210, 211, or 212).
    pub code: u16,
    /// 256-bit tin ID in hex.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
    /// Descriptive status message.
    pub message: String,
}

/// Result of storing a tin payload after upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreResult {
    /// 256-bit tin ID.
    #[serde(with = "hex_tin_id")]
    pub tin_id: TinId,
    /// Owner account ID.
    pub owner: u64,
    /// Whether the file was written to disk (`true`), or discarded into `/dev/null` (`false`).
    pub written_to_disk: bool,
    /// Whether a new DB row was registered.
    pub reference_added: bool,
}

/// Unregister request to remove ownership of a tin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnregisterRequest {
    /// Owner account ID.
    pub owner: u64,
    /// Active session ID.
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
