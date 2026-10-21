//! The core `Tin` data structure and primitive helper methods.

pub mod float;
pub mod int;
pub mod label;
mod opener;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::TinId;
use crate::codec::{
    decode_raw_plaintext_bytes, encode_raw_convergent, encode_raw_plaintext, encode_raw_room_key,
};
use crate::error::TinError;
use crate::tin::label::LABEL_PRIMITIVE_STRING;
use crate::tin::opener::Opener;

use label::{LABEL_ALHALTE_TIN_ID, LABEL_PRIMITIVE_BOOL, Label, label_hash};

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

pub struct Tin {
    pub opener: Opener,
    pub content: TinContent,
}

impl From<String> for TinContent {
    fn from(value: String) -> Self {
        Self::new(LABEL_PRIMITIVE_STRING, value.into_bytes())
    }
}

impl From<bool> for TinContent {
    fn from(value: bool) -> Self {
        Self::new(LABEL_PRIMITIVE_BOOL, vec![if value { 1 } else { 0 }])
    }
}

impl TryInto<bool> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<bool, Self::Error> {
        if self.label != LABEL_PRIMITIVE_BOOL {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_BOOL,
                actual: self.label,
            });
        }
        if self.payload.len() != 1 {
            return Err(TinError::InvalidPayloadLength {
                expected: 1,
                actual: self.payload.len(),
            });
        }
        match self.payload[0] {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(TinError::InvalidBool(b)),
        }
    }
}

impl TryInto<String> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<String, Self::Error> {
        if self.label != LABEL_PRIMITIVE_STRING {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_STRING,
                actual: self.label,
            });
        }
        String::from_utf8(self.payload).map_err(|err| TinError::InvalidUtf8(err))
    }
}

impl Into<TinContent> for TinId {
    fn into(self) -> TinContent {
        TinContent::new(LABEL_ALHALTE_TIN_ID, self.to_vec())
    }
}

impl TryFrom<TinContent> for TinId {
    type Error = TinError;

    fn try_from(value: TinContent) -> Result<Self, Self::Error> {
        if value.label != LABEL_ALHALTE_TIN_ID {
            return Err(TinError::LabelMismatch {
                expected: LABEL_ALHALTE_TIN_ID,
                actual: value.label,
            });
        }
        if value.payload.len() != 32 {
            return Err(TinError::InvalidPayloadLength {
                expected: 32,
                actual: value.payload.len(),
            });
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&value.payload);
        Ok(buf)
    }
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
    pub fn to_plaintext(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.payload.len());
        out.extend_from_slice(&self.label.to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Parses a `Tin` from uncompressed plaintext bytes: `label(8B, u64 LE) || payload`.
    pub fn from_plaintext(bytes: &[u8]) -> Result<Self, TinError> {
        if bytes.len() < 8 {
            return Err(TinError::PlaintextTooShort(bytes.len()));
        }
        let mut label_buf = [0u8; 8];
        label_buf.copy_from_slice(&bytes[0..8]);
        let label = u64::from_le_bytes(label_buf);
        let payload = bytes[8..].to_vec();
        Ok(Self { label, payload })
    }

    /// Encodes this `Tin` as an unencrypted wire-format tin (`0000`).
    ///
    /// Returns `(wire_bytes, tin_id)`.
    pub fn encode_plaintext(&self) -> Result<Self, TinError> {
        let plaintext = self.content.to_plaintext();
        encode_raw_plaintext(&plaintext, self.opener.compression)
    }

    /// Encodes this `Tin` using convergent encryption (`0001`).
    ///
    /// Returns `(wire_bytes, tin_id, convergent_key)`.
    pub fn encode_convergent(&self) -> Result<(Vec<u8>, TinId, [u8; 32]), TinError> {
        let plaintext = self.content.to_plaintext();
        encode_raw_convergent(&plaintext, self.opener.compression)
    }

    /// Encodes this `Tin` using room key encryption (`0010`).
    ///
    /// Returns `(wire_bytes, tin_id)`.
    pub fn encode_room_key(
        &self,
        room_key: &[u8; 32],
        nonce: &[u8; 12],
    ) -> Result<(Vec<u8>, TinId), TinError> {
        let plaintext = self.content.to_plaintext();
        encode_raw_room_key(&plaintext, self.opener.compression, room_key, nonce)
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
