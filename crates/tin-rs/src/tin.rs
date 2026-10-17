//! The core `Tin` data structure and primitive helper methods.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::codec::{
    decode_raw_plaintext_bytes, encode_raw_convergent, encode_raw_plaintext, encode_raw_room_key,
};
use crate::encryption::TinId;
use crate::error::TinError;
use crate::label::{
    LABEL_ALHALTE_TIN_ID, LABEL_ALHALTE_UNIX_TIME, LABEL_NEED_NOT_KNOW, LABEL_PRIMITIVE_BOOL,
    LABEL_PRIMITIVE_F16, LABEL_PRIMITIVE_F32, LABEL_PRIMITIVE_F64, LABEL_PRIMITIVE_I8,
    LABEL_PRIMITIVE_I16, LABEL_PRIMITIVE_I32, LABEL_PRIMITIVE_I64, LABEL_PRIMITIVE_I128,
    LABEL_PRIMITIVE_STRING, LABEL_PRIMITIVE_U8, LABEL_PRIMITIVE_U16, LABEL_PRIMITIVE_U32,
    LABEL_PRIMITIVE_U64, LABEL_PRIMITIVE_U128, Label, label_hash,
};
use crate::opener::{Compression, Opener};

/// Self-typed binary container in the Alhalte ecosystem.
///
/// A `Tin` contains a 64-bit format identifier (`label`) and arbitrary binary `payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tin {
    /// 64-bit XXH64 hash identifier of the data format.
    pub label: Label,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
}

macro_rules! impl_int_codec {
    ($type:ty, $from_fn:ident, $as_fn:ident, $label:ident, $size:expr) => {
        #[doc = concat!("Creates a new `Tin` containing a `", stringify!($type), "` value encoded as little-endian.")]
        pub fn $from_fn(val: $type) -> Self {
            Self::new($label, val.to_le_bytes().to_vec())
        }

        #[doc = concat!("Decodes the payload as a little-endian `", stringify!($type), "` value.")]
        pub fn $as_fn(&self) -> Result<$type, TinError> {
            if self.label != $label {
                return Err(TinError::LabelMismatch {
                    expected: $label,
                    actual: self.label,
                });
            }
            if self.payload.len() != $size {
                return Err(TinError::InvalidPayloadLength {
                    expected: $size,
                    actual: self.payload.len(),
                });
            }
            let mut buf = [0u8; $size];
            buf.copy_from_slice(&self.payload);
            Ok(<$type>::from_le_bytes(buf))
        }
    };
}

impl Tin {
    /// Creates a new `Tin` with the specified numeric label and payload.
    pub const fn new(label: Label, payload: Vec<u8>) -> Self {
        Self { label, payload }
    }

    /// Creates a new `Tin` using a format identifier string hashed with XXH64.
    pub fn with_ident(ident: &str, payload: Vec<u8>) -> Self {
        Self::new(label_hash(ident), payload)
    }

    /// Creates a new `Tin` with the `need not know` label (`0`).
    pub const fn need_not_know(payload: Vec<u8>) -> Self {
        Self::new(LABEL_NEED_NOT_KNOW, payload)
    }

    /// Creates a new `Tin` containing a UTF-8 string with `primitive/string` label.
    pub fn from_string(s: &str) -> Self {
        Self::new(LABEL_PRIMITIVE_STRING, s.as_bytes().to_vec())
    }

    /// Decodes the payload as a UTF-8 string slice.
    pub fn as_string(&self) -> Result<&str, TinError> {
        if self.label != LABEL_PRIMITIVE_STRING {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_STRING,
                actual: self.label,
            });
        }
        std::str::from_utf8(&self.payload).map_err(TinError::InvalidUtf8)
    }

    impl_int_codec!(u8, from_u8, as_u8, LABEL_PRIMITIVE_U8, 1);
    impl_int_codec!(u16, from_u16, as_u16, LABEL_PRIMITIVE_U16, 2);
    impl_int_codec!(u32, from_u32, as_u32, LABEL_PRIMITIVE_U32, 4);
    impl_int_codec!(u64, from_u64, as_u64, LABEL_PRIMITIVE_U64, 8);
    impl_int_codec!(u128, from_u128, as_u128, LABEL_PRIMITIVE_U128, 16);

    impl_int_codec!(i8, from_i8, as_i8, LABEL_PRIMITIVE_I8, 1);
    impl_int_codec!(i16, from_i16, as_i16, LABEL_PRIMITIVE_I16, 2);
    impl_int_codec!(i32, from_i32, as_i32, LABEL_PRIMITIVE_I32, 4);
    impl_int_codec!(i64, from_i64, as_i64, LABEL_PRIMITIVE_I64, 8);
    impl_int_codec!(i128, from_i128, as_i128, LABEL_PRIMITIVE_I128, 16);

    /// Creates a new `Tin` containing an IEEE 754 16-bit half-precision float (`primitive/f16`).
    pub fn from_f16(val: half::f16) -> Self {
        Self::new(LABEL_PRIMITIVE_F16, val.to_le_bytes().to_vec())
    }

    /// Decodes the payload as an IEEE 754 16-bit half-precision float.
    pub fn as_f16(&self) -> Result<half::f16, TinError> {
        if self.label != LABEL_PRIMITIVE_F16 {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_F16,
                actual: self.label,
            });
        }
        if self.payload.len() != 2 {
            return Err(TinError::InvalidPayloadLength {
                expected: 2,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 2];
        buf.copy_from_slice(&self.payload);
        Ok(half::f16::from_le_bytes(buf))
    }

    /// Creates a new `Tin` containing an IEEE 754 32-bit single-precision float (`primitive/f32`).
    pub fn from_f32(val: f32) -> Self {
        Self::new(LABEL_PRIMITIVE_F32, val.to_le_bytes().to_vec())
    }

    /// Decodes the payload as an IEEE 754 32-bit single-precision float.
    pub fn as_f32(&self) -> Result<f32, TinError> {
        if self.label != LABEL_PRIMITIVE_F32 {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_F32,
                actual: self.label,
            });
        }
        if self.payload.len() != 4 {
            return Err(TinError::InvalidPayloadLength {
                expected: 4,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.payload);
        Ok(f32::from_le_bytes(buf))
    }

    /// Creates a new `Tin` containing an IEEE 754 64-bit double-precision float (`primitive/f64`).
    pub fn from_f64(val: f64) -> Self {
        Self::new(LABEL_PRIMITIVE_F64, val.to_le_bytes().to_vec())
    }

    /// Decodes the payload as an IEEE 754 64-bit double-precision float.
    pub fn as_f64(&self) -> Result<f64, TinError> {
        if self.label != LABEL_PRIMITIVE_F64 {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_F64,
                actual: self.label,
            });
        }
        if self.payload.len() != 8 {
            return Err(TinError::InvalidPayloadLength {
                expected: 8,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.payload);
        Ok(f64::from_le_bytes(buf))
    }

    /// Creates a new `Tin` containing a boolean (`primitive/bool`).
    pub fn from_bool(val: bool) -> Self {
        Self::new(LABEL_PRIMITIVE_BOOL, vec![if val { 1 } else { 0 }])
    }

    /// Decodes the payload as a boolean.
    pub fn as_bool(&self) -> Result<bool, TinError> {
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

    /// Creates a new `Tin` containing 64-bit unsigned UNIX time (`alhalte/unix-time`).
    pub fn from_unix_time(secs: u64) -> Self {
        Self::new(LABEL_ALHALTE_UNIX_TIME, secs.to_le_bytes().to_vec())
    }

    /// Decodes the payload as 64-bit unsigned UNIX time.
    pub fn as_unix_time(&self) -> Result<u64, TinError> {
        if self.label != LABEL_ALHALTE_UNIX_TIME {
            return Err(TinError::LabelMismatch {
                expected: LABEL_ALHALTE_UNIX_TIME,
                actual: self.label,
            });
        }
        if self.payload.len() != 8 {
            return Err(TinError::InvalidPayloadLength {
                expected: 8,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.payload);
        Ok(u64::from_le_bytes(buf))
    }

    /// Creates a new `Tin` containing a 256-bit tin ID (`alhalte/tin-id`).
    pub fn from_tin_id(id: TinId) -> Self {
        Self::new(LABEL_ALHALTE_TIN_ID, id.to_vec())
    }

    /// Decodes the payload as a 256-bit tin ID.
    pub fn as_tin_id(&self) -> Result<TinId, TinError> {
        if self.label != LABEL_ALHALTE_TIN_ID {
            return Err(TinError::LabelMismatch {
                expected: LABEL_ALHALTE_TIN_ID,
                actual: self.label,
            });
        }
        if self.payload.len() != 32 {
            return Err(TinError::InvalidPayloadLength {
                expected: 32,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&self.payload);
        Ok(buf)
    }

    /// Creates a new `Tin` by serializing `val` using `postcard` and setting label to `ident` hash.
    pub fn from_postcard<T: Serialize>(ident: &str, val: &T) -> Result<Self, TinError> {
        Self::from_postcard_label(label_hash(ident), val)
    }

    /// Creates a new `Tin` by serializing `val` using `postcard` with the given numeric label.
    pub fn from_postcard_label<T: Serialize>(label: Label, val: &T) -> Result<Self, TinError> {
        let payload = postcard::to_allocvec(val)?;
        Ok(Self::new(label, payload))
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
    pub fn encode_plaintext(&self, compression: Compression) -> Result<(Vec<u8>, TinId), TinError> {
        let plaintext = self.to_plaintext();
        encode_raw_plaintext(&plaintext, compression)
    }

    /// Encodes this `Tin` using convergent encryption (`0001`).
    ///
    /// Returns `(wire_bytes, tin_id, convergent_key)`.
    pub fn encode_convergent(
        &self,
        compression: Compression,
    ) -> Result<(Vec<u8>, TinId, [u8; 32]), TinError> {
        let plaintext = self.to_plaintext();
        encode_raw_convergent(&plaintext, compression)
    }

    /// Encodes this `Tin` using room key encryption (`0010`).
    ///
    /// Returns `(wire_bytes, tin_id)`.
    pub fn encode_room_key(
        &self,
        compression: Compression,
        room_key: &[u8; 32],
        nonce: &[u8; 12],
    ) -> Result<(Vec<u8>, TinId), TinError> {
        let plaintext = self.to_plaintext();
        encode_raw_room_key(&plaintext, compression, room_key, nonce)
    }

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
