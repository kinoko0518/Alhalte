//! Label type, hashing function, and standard label identifiers.

/// 64-bit identifier representing the format or type of tin payload.
pub type Label = u64;

/// Computes the 64-bit XXH64 hash with seed 0 for a given identifier string.
#[inline]
pub fn label_hash(ident: &str) -> Label {
    xxhash_rust::xxh64::xxh64(ident.as_bytes(), 0)
}

/// Constant for "need not know" label whose hash is 0.
pub const LABEL_NEED_NOT_KNOW: Label = 0;

/// Identifier string for UTF-8 string primitive (`primitive/string`).
pub const IDENT_PRIMITIVE_STRING: &str = "primitive/string";
/// Identifier string for 8-bit unsigned integer primitive (`primitive/u8`).
pub const IDENT_PRIMITIVE_U8: &str = "primitive/u8";
/// Identifier string for 16-bit unsigned integer primitive (`primitive/u16`).
pub const IDENT_PRIMITIVE_U16: &str = "primitive/u16";
/// Identifier string for 32-bit unsigned integer primitive (`primitive/u32`).
pub const IDENT_PRIMITIVE_U32: &str = "primitive/u32";
/// Identifier string for 64-bit unsigned integer primitive (`primitive/u64`).
pub const IDENT_PRIMITIVE_U64: &str = "primitive/u64";
/// Identifier string for 128-bit unsigned integer primitive (`primitive/u128`).
pub const IDENT_PRIMITIVE_U128: &str = "primitive/u128";

/// Identifier string for 8-bit signed integer primitive (`primitive/i8`).
pub const IDENT_PRIMITIVE_I8: &str = "primitive/i8";
/// Identifier string for 16-bit signed integer primitive (`primitive/i16`).
pub const IDENT_PRIMITIVE_I16: &str = "primitive/i16";
/// Identifier string for 32-bit signed integer primitive (`primitive/i32`).
pub const IDENT_PRIMITIVE_I32: &str = "primitive/i32";
/// Identifier string for 64-bit signed integer primitive (`primitive/i64`).
pub const IDENT_PRIMITIVE_I64: &str = "primitive/i64";
/// Identifier string for 128-bit signed integer primitive (`primitive/i128`).
pub const IDENT_PRIMITIVE_I128: &str = "primitive/i128";

/// Identifier string for 16-bit floating-point primitive (`primitive/f16`).
pub const IDENT_PRIMITIVE_F16: &str = "primitive/f16";
/// Identifier string for 32-bit floating-point primitive (`primitive/f32`).
pub const IDENT_PRIMITIVE_F32: &str = "primitive/f32";
/// Identifier string for 64-bit floating-point primitive (`primitive/f64`).
pub const IDENT_PRIMITIVE_F64: &str = "primitive/f64";

/// Identifier string for boolean primitive (`primitive/bool`).
pub const IDENT_PRIMITIVE_BOOL: &str = "primitive/bool";

/// Identifier string for 64-bit UNIX time (`alhalte/unix-time`).
pub const IDENT_ALHALTE_UNIX_TIME: &str = "alhalte/unix-time";
/// Identifier string for 256-bit tin ID (`alhalte/tin-id`).
pub const IDENT_ALHALTE_TIN_ID: &str = "alhalte/tin-id";

/// Precomputed label for `primitive/string`.
pub const LABEL_PRIMITIVE_STRING: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_STRING.as_bytes(), 0);
/// Precomputed label for `primitive/u8`.
pub const LABEL_PRIMITIVE_U8: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_U8.as_bytes(), 0);
/// Precomputed label for `primitive/u16`.
pub const LABEL_PRIMITIVE_U16: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_U16.as_bytes(), 0);
/// Precomputed label for `primitive/u32`.
pub const LABEL_PRIMITIVE_U32: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_U32.as_bytes(), 0);
/// Precomputed label for `primitive/u64`.
pub const LABEL_PRIMITIVE_U64: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_U64.as_bytes(), 0);
/// Precomputed label for `primitive/u128`.
pub const LABEL_PRIMITIVE_U128: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_U128.as_bytes(), 0);

/// Precomputed label for `primitive/i8`.
pub const LABEL_PRIMITIVE_I8: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_I8.as_bytes(), 0);
/// Precomputed label for `primitive/i16`.
pub const LABEL_PRIMITIVE_I16: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_I16.as_bytes(), 0);
/// Precomputed label for `primitive/i32`.
pub const LABEL_PRIMITIVE_I32: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_I32.as_bytes(), 0);
/// Precomputed label for `primitive/i64`.
pub const LABEL_PRIMITIVE_I64: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_I64.as_bytes(), 0);
/// Precomputed label for `primitive/i128`.
pub const LABEL_PRIMITIVE_I128: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_I128.as_bytes(), 0);

/// Precomputed label for `primitive/f16`.
pub const LABEL_PRIMITIVE_F16: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_F16.as_bytes(), 0);
/// Precomputed label for `primitive/f32`.
pub const LABEL_PRIMITIVE_F32: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_F32.as_bytes(), 0);
/// Precomputed label for `primitive/f64`.
pub const LABEL_PRIMITIVE_F64: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_F64.as_bytes(), 0);

/// Precomputed label for `primitive/bool`.
pub const LABEL_PRIMITIVE_BOOL: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_PRIMITIVE_BOOL.as_bytes(), 0);

/// Precomputed label for `alhalte/unix-time`.
pub const LABEL_ALHALTE_UNIX_TIME: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_UNIX_TIME.as_bytes(), 0);
/// Precomputed label for `alhalte/tin-id`.
pub const LABEL_ALHALTE_TIN_ID: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_TIN_ID.as_bytes(), 0);
