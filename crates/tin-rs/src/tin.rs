//! The core `Tin` data structure and primitive helper methods.

mod convertion;
pub mod float;
pub mod int;
pub mod label;
mod opener;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::TinId;
use crate::codec::{decode_raw_plaintext_bytes, encode_raw_convergent};
use crate::error::TinError;
use crate::tin::opener::{Compression, ConvergentKey, Encryption, Opener, compress};

use label::{Label, label_hash};

/// Self-typed binary container in the Alhalte ecosystem.
///
/// A `Tin` contains a 64-bit format identifier (`label`) and arbitrary binary `payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TinContent {
    /// 64-bit XXH64 hash identifier of the data format.
    pub label: Label,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
}

enum Encrypter {
    Plain,
    Convergent { convergent_key: ConvergentKey },
    RoomId { nonce: [u8; 12], room_key: [u8; 32] },
}

pub struct Tin {
    // content
    tin_id: TinId,
    content: Vec<u8>,

    // opener
    compression: Compression,
    encryption: Encrypter,
}

trait Tinnable {
    const LABEL: Label;

    /// Creates a new `Tin` by serializing `val` using `postcard` with the given numeric label.
    fn from_postcard<T: Serialize>(val: &T) -> Result<TinContent, TinError> {
        let payload = postcard::to_allocvec(val)?;
        Ok(TinContent::new(Self::LABEL, payload))
    }
}

impl TinContent {
    /// Creates a new `Tin` with the specified numeric label and payload.
    ///
    /// Payloads mustn't be encrypted or compressed.
    pub const fn new(label: Label, payload: Vec<u8>) -> Self {
        Self { label, payload }
    }

    /// Creates a new `Tin` using a format identifier string hashed with XXH64.
    pub fn with_ident(ident: &str, payload: Vec<u8>) -> Self {
        Self::new(label_hash(ident), payload)
    }

    /// Deserializes the payload into `T` using `postcard`.
    pub fn to_postcard<T: DeserializeOwned>(&self) -> Result<T, TinError> {
        postcard::from_bytes(&self.payload).map_err(TinError::Postcard)
    }

    /// Converts this `Tin` into uncompressed plaintext representation: `label(8B, u64 LE) || payload`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.payload.len());
        out.extend_from_slice(&self.label.to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Parses a `Tin` from uncompressed plaintext bytes: `label(8B, u64 LE) || payload`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TinError> {
        if bytes.len() < 8 {
            return Err(TinError::PlaintextTooShort(bytes.len()));
        }
        let mut label_buf = [0u8; 8];
        label_buf.copy_from_slice(&bytes[0..8]);
        let label = u64::from_le_bytes(label_buf);
        let payload = bytes[8..].to_vec();
        Ok(Self { label, payload })
    }
}

impl Tin {
    /// Decodes a `Tin` from wire format bytes.
    ///
    /// Returns `(tin, opener, tin_id)`.
    pub fn decode(
        wire_bytes: &[u8],
        key: Option<&[u8; 32]>,
    ) -> Result<(Self, Opener, TinId), TinError> {
        let (raw, opener, tin_id) = decode_raw_plaintext_bytes(wire_bytes, key)?;
        let tin = Self::from_plaintext(&raw)?;
        Ok((tin, opener, tin_id))
    }
}
