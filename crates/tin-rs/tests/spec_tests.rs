//! Specification compliance tests for `tin-rs`.
//!
//! Each test references the corresponding section of `ARCHITECTURE.md`.

use tin::{
    Compression, Encryption, EncryptionParam, IDENT_ALHALTE_TIN_ID, IDENT_ALHALTE_UNIX_TIME,
    IDENT_PRIMITIVE_BOOL, IDENT_PRIMITIVE_F16, IDENT_PRIMITIVE_F32, IDENT_PRIMITIVE_F64,
    IDENT_PRIMITIVE_I8, IDENT_PRIMITIVE_I16, IDENT_PRIMITIVE_I32, IDENT_PRIMITIVE_I64,
    IDENT_PRIMITIVE_I128, IDENT_PRIMITIVE_STRING, IDENT_PRIMITIVE_U8, IDENT_PRIMITIVE_U16,
    IDENT_PRIMITIVE_U32, IDENT_PRIMITIVE_U64, IDENT_PRIMITIVE_U128, LABEL_ALHALTE_TIN_ID,
    LABEL_ALHALTE_UNIX_TIME, LABEL_NEED_NOT_KNOW, LABEL_PRIMITIVE_BOOL, LABEL_PRIMITIVE_F16,
    LABEL_PRIMITIVE_F32, LABEL_PRIMITIVE_F64, LABEL_PRIMITIVE_I8, LABEL_PRIMITIVE_I16,
    LABEL_PRIMITIVE_I32, LABEL_PRIMITIVE_I64, LABEL_PRIMITIVE_I128, LABEL_PRIMITIVE_STRING,
    LABEL_PRIMITIVE_U8, LABEL_PRIMITIVE_U16, LABEL_PRIMITIVE_U32, LABEL_PRIMITIVE_U64,
    LABEL_PRIMITIVE_U128, Opener, Tin, TinError, TinId, decode, decode_raw_plaintext_bytes,
    encode_raw_plaintext_bytes, label_hash, verify_convergent,
};

#[test]
// spec: tin-rs/opener 0000 plaintext
fn test_opener_plaintext() {
    let compressions = [
        (Compression::None, 0x00),
        (Compression::Zstd, 0x01),
        (Compression::Lz4, 0x02),
        (Compression::Brotli, 0x03),
    ];

    for (comp, byte_val) in compressions {
        let opener = Opener::new(Encryption::Plaintext, comp);
        assert_eq!(opener.to_byte(), byte_val);
        assert_eq!(Opener::from_byte(byte_val).unwrap(), opener);
        assert_eq!(u8::from(opener), byte_val);
        assert_eq!(Opener::try_from(byte_val).unwrap(), opener);
        assert!(!opener.encryption.has_wire_nonce());
    }
}

#[test]
// spec: tin-rs/opener 0001 convergent
fn test_opener_convergent() {
    let compressions = [
        (Compression::None, 0x10),
        (Compression::Zstd, 0x11),
        (Compression::Lz4, 0x12),
        (Compression::Brotli, 0x13),
    ];

    for (comp, byte_val) in compressions {
        let opener = Opener::new(Encryption::Convergent, comp);
        assert_eq!(opener.to_byte(), byte_val);
        assert_eq!(Opener::from_byte(byte_val).unwrap(), opener);
        assert!(!opener.encryption.has_wire_nonce());
    }
}

#[test]
// spec: tin-rs/opener 0010 nonce
fn test_opener_room_key_nonce() {
    let compressions = [
        (Compression::None, 0x20),
        (Compression::Zstd, 0x21),
        (Compression::Lz4, 0x22),
        (Compression::Brotli, 0x23),
    ];

    for (comp, byte_val) in compressions {
        let opener = Opener::new(Encryption::RoomKey, comp);
        assert_eq!(opener.to_byte(), byte_val);
        assert_eq!(Opener::from_byte(byte_val).unwrap(), opener);
        assert!(opener.encryption.has_wire_nonce());
    }
}

#[test]
// spec: tin-rs/opener unsupported encryption
fn test_opener_unsupported_encryption() {
    for enc in 0x3..=0x0F {
        let byte = enc << 4;
        let err = Opener::from_byte(byte).unwrap_err();
        assert_eq!(err, TinError::UnsupportedEncryption(enc));
    }
}

#[test]
// spec: tin-rs/opener unsupported compression
fn test_opener_unsupported_compression() {
    for comp in 0x4..=0x0F {
        let byte = comp;
        let err = Opener::from_byte(byte).unwrap_err();
        assert_eq!(err, TinError::UnsupportedCompression(comp));
    }
}

#[test]
// spec: tin-rs/opener compression 0000 none
// spec: tin-rs/opener compression 0001 zstd
// spec: tin-rs/opener compression 0010 lz4
// spec: tin-rs/opener compression 0011 brotli
fn test_compressions_round_trip() {
    let sample = b"Alhalte self-hosted VTT system - testing compression algorithms with repeated patterns: abcdabcdabcdabcdabcd1234567890.";

    for comp in [
        Compression::None,
        Compression::Zstd,
        Compression::Lz4,
        Compression::Brotli,
    ] {
        let compressed = tin::compress(sample, comp).unwrap();
        let decompressed = tin::decompress(&compressed, comp).unwrap();
        assert_eq!(
            decompressed, sample,
            "Compression {:?} failed round-trip",
            comp
        );
    }
}

#[test]
// spec: tin-rs/tin-id convergent xof 512
fn test_convergent_tin_id_and_key() {
    let tin = Tin::from_string("Deterministic Convergent Encryption Test Payload");
    let (wire_bytes, tin_id, conv_key) = tin.encode_convergent(Compression::Zstd).unwrap();

    // Verify wire format: opener (1B) || body (no nonce in wire)
    let opener = Opener::from_byte(wire_bytes[0]).unwrap();
    assert_eq!(opener.encryption, Encryption::Convergent);
    assert_eq!(opener.compression, Compression::Zstd);

    // Verify TinId matches BLAKE3 XOF 512 first 256 bits of plaintext
    let plaintext = tin.to_plaintext();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&plaintext);
    let mut xof = hasher.finalize_xof();
    let mut xof_out = [0u8; 64];
    xof.fill(&mut xof_out);
    assert_eq!(&tin_id[..], &xof_out[0..32]);
    assert_eq!(&conv_key[..], &xof_out[32..64]);

    // Test verification helper
    verify_convergent(&plaintext, &conv_key, Some(&tin_id)).unwrap();

    // Decoding with correct key succeeds
    let (decoded_tin, decoded_opener, decoded_id) =
        Tin::decode(&wire_bytes, Some(&conv_key)).unwrap();
    assert_eq!(decoded_tin, tin);
    assert_eq!(decoded_opener, opener);
    assert_eq!(decoded_id, tin_id);

    // Decoding without key fails with MissingKey
    let err = Tin::decode(&wire_bytes, None).unwrap_err();
    assert_eq!(err, TinError::MissingKey);

    // Decoding with wrong key fails
    let wrong_key = [0x42u8; 32];
    let err = Tin::decode(&wire_bytes, Some(&wrong_key)).unwrap_err();
    assert!(matches!(
        err,
        TinError::DecryptionFailed(_) | TinError::KeyMismatch
    ));
}

#[test]
// spec: tin-rs/tin-id plaintext and roomkey blake3-256
fn test_tin_id_plaintext_and_room_key() {
    let tin = Tin::from_string("Testing TinId for Plaintext and RoomKey");

    // Plaintext: TinId is blake3-256 of entire wire_bytes
    let (plain_bytes, plain_id) = tin.encode_plaintext(Compression::Lz4).unwrap();
    let expected_plain_id = *blake3::hash(&plain_bytes).as_bytes();
    assert_eq!(plain_id, expected_plain_id);

    let (decoded_plain, plain_opener, decoded_plain_id) = Tin::decode(&plain_bytes, None).unwrap();
    assert_eq!(decoded_plain, tin);
    assert_eq!(plain_opener.encryption, Encryption::Plaintext);
    assert_eq!(decoded_plain_id, expected_plain_id);

    // RoomKey: TinId is blake3-256 of entire wire_bytes
    let room_key = [0x55u8; 32];
    let nonce = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let (room_bytes, room_id) = tin
        .encode_room_key(Compression::Brotli, &room_key, &nonce)
        .unwrap();
    let expected_room_id = *blake3::hash(&room_bytes).as_bytes();
    assert_eq!(room_id, expected_room_id);

    // Wire format must have opener(1B) || nonce(12B) || body
    assert_eq!(
        room_bytes[0],
        Opener::new(Encryption::RoomKey, Compression::Brotli).to_byte()
    );
    assert_eq!(&room_bytes[1..13], &nonce);

    let (decoded_room, room_opener, decoded_room_id) =
        Tin::decode(&room_bytes, Some(&room_key)).unwrap();
    assert_eq!(decoded_room, tin);
    assert_eq!(room_opener.encryption, Encryption::RoomKey);
    assert_eq!(decoded_room_id, expected_room_id);

    // Missing key
    let err = Tin::decode(&room_bytes, None).unwrap_err();
    assert_eq!(err, TinError::MissingKey);

    // Wrong key
    let wrong_key = [0x99u8; 32];
    let err = Tin::decode(&room_bytes, Some(&wrong_key)).unwrap_err();
    assert!(matches!(err, TinError::DecryptionFailed(_)));
}

#[test]
// spec: tin-rs/label xxh64 and need not know
fn test_label_hash_and_constants() {
    assert_eq!(LABEL_NEED_NOT_KNOW, 0);

    // Verify compile-time constants equal runtime label_hash
    assert_eq!(label_hash(IDENT_PRIMITIVE_STRING), LABEL_PRIMITIVE_STRING);
    assert_eq!(label_hash(IDENT_PRIMITIVE_U8), LABEL_PRIMITIVE_U8);
    assert_eq!(label_hash(IDENT_PRIMITIVE_U16), LABEL_PRIMITIVE_U16);
    assert_eq!(label_hash(IDENT_PRIMITIVE_U32), LABEL_PRIMITIVE_U32);
    assert_eq!(label_hash(IDENT_PRIMITIVE_U64), LABEL_PRIMITIVE_U64);
    assert_eq!(label_hash(IDENT_PRIMITIVE_U128), LABEL_PRIMITIVE_U128);

    assert_eq!(label_hash(IDENT_PRIMITIVE_I8), LABEL_PRIMITIVE_I8);
    assert_eq!(label_hash(IDENT_PRIMITIVE_I16), LABEL_PRIMITIVE_I16);
    assert_eq!(label_hash(IDENT_PRIMITIVE_I32), LABEL_PRIMITIVE_I32);
    assert_eq!(label_hash(IDENT_PRIMITIVE_I64), LABEL_PRIMITIVE_I64);
    assert_eq!(label_hash(IDENT_PRIMITIVE_I128), LABEL_PRIMITIVE_I128);

    assert_eq!(label_hash(IDENT_PRIMITIVE_F16), LABEL_PRIMITIVE_F16);
    assert_eq!(label_hash(IDENT_PRIMITIVE_F32), LABEL_PRIMITIVE_F32);
    assert_eq!(label_hash(IDENT_PRIMITIVE_F64), LABEL_PRIMITIVE_F64);

    assert_eq!(label_hash(IDENT_PRIMITIVE_BOOL), LABEL_PRIMITIVE_BOOL);
    assert_eq!(label_hash(IDENT_ALHALTE_UNIX_TIME), LABEL_ALHALTE_UNIX_TIME);
    assert_eq!(label_hash(IDENT_ALHALTE_TIN_ID), LABEL_ALHALTE_TIN_ID);
}

#[test]
// spec: tin-rs/label primitive string
fn test_primitive_string() {
    let s = "Hello, Alhalte!";
    let tin = Tin::from_string(s);
    assert_eq!(tin.label, LABEL_PRIMITIVE_STRING);
    assert_eq!(tin.as_string().unwrap(), s);

    // Label mismatch test
    let wrong_tin = Tin::new(LABEL_NEED_NOT_KNOW, s.as_bytes().to_vec());
    assert_eq!(
        wrong_tin.as_string().unwrap_err(),
        TinError::LabelMismatch {
            expected: LABEL_PRIMITIVE_STRING,
            actual: LABEL_NEED_NOT_KNOW
        }
    );

    // Invalid UTF-8 test
    let bad_utf8_tin = Tin::new(LABEL_PRIMITIVE_STRING, vec![0xFF, 0xFF]);
    assert!(matches!(
        bad_utf8_tin.as_string(),
        Err(TinError::InvalidUtf8(_))
    ));
}

#[test]
// spec: tin-rs/label primitive integers u8-u128 i8-i128
fn test_primitive_integers() {
    // u8
    let tin = Tin::from_u8(250);
    assert_eq!(tin.label, LABEL_PRIMITIVE_U8);
    assert_eq!(tin.as_u8().unwrap(), 250);

    // u16
    let tin = Tin::from_u16(0x1234);
    assert_eq!(tin.label, LABEL_PRIMITIVE_U16);
    assert_eq!(tin.as_u16().unwrap(), 0x1234);

    // u32
    let tin = Tin::from_u32(0x12345678);
    assert_eq!(tin.label, LABEL_PRIMITIVE_U32);
    assert_eq!(tin.as_u32().unwrap(), 0x12345678);

    // u64
    let tin = Tin::from_u64(0x1234567890ABCDEF);
    assert_eq!(tin.label, LABEL_PRIMITIVE_U64);
    assert_eq!(tin.as_u64().unwrap(), 0x1234567890ABCDEF);

    // u128
    let tin = Tin::from_u128(0x1234567890ABCDEF1122334455667788);
    assert_eq!(tin.label, LABEL_PRIMITIVE_U128);
    assert_eq!(tin.as_u128().unwrap(), 0x1234567890ABCDEF1122334455667788);

    // i8
    let tin = Tin::from_i8(-42);
    assert_eq!(tin.label, LABEL_PRIMITIVE_I8);
    assert_eq!(tin.as_i8().unwrap(), -42);

    // i16
    let tin = Tin::from_i16(-1234);
    assert_eq!(tin.label, LABEL_PRIMITIVE_I16);
    assert_eq!(tin.as_i16().unwrap(), -1234);

    // i32
    let tin = Tin::from_i32(-12345678);
    assert_eq!(tin.label, LABEL_PRIMITIVE_I32);
    assert_eq!(tin.as_i32().unwrap(), -12345678);

    // i64
    let tin = Tin::from_i64(-1234567890123456);
    assert_eq!(tin.label, LABEL_PRIMITIVE_I64);
    assert_eq!(tin.as_i64().unwrap(), -1234567890123456);

    // i128
    let tin = Tin::from_i128(-123456789012345678901234567890);
    assert_eq!(tin.label, LABEL_PRIMITIVE_I128);
    assert_eq!(tin.as_i128().unwrap(), -123456789012345678901234567890);

    // Length check error
    let bad_len_tin = Tin::new(LABEL_PRIMITIVE_U32, vec![1, 2]);
    assert_eq!(
        bad_len_tin.as_u32().unwrap_err(),
        TinError::InvalidPayloadLength {
            expected: 4,
            actual: 2
        }
    );
}

#[test]
// spec: tin-rs/label primitive floats f16 f32 f64
fn test_primitive_floats() {
    // f16
    let h = half::f16::from_f32(3.140625);
    let tin = Tin::from_f16(h);
    assert_eq!(tin.label, LABEL_PRIMITIVE_F16);
    assert_eq!(tin.as_f16().unwrap(), h);

    // f32
    let f = std::f32::consts::PI;
    let tin = Tin::from_f32(f);
    assert_eq!(tin.label, LABEL_PRIMITIVE_F32);
    assert_eq!(tin.as_f32().unwrap(), f);

    // f64
    let d = std::f64::consts::E;
    let tin = Tin::from_f64(d);
    assert_eq!(tin.label, LABEL_PRIMITIVE_F64);
    assert_eq!(tin.as_f64().unwrap(), d);
}

#[test]
// spec: tin-rs/label primitive bool
fn test_primitive_bool() {
    let t_true = Tin::from_bool(true);
    assert_eq!(t_true.label, LABEL_PRIMITIVE_BOOL);
    assert_eq!(t_true.payload, vec![1]);
    assert!(t_true.as_bool().unwrap());

    let t_false = Tin::from_bool(false);
    assert_eq!(t_false.label, LABEL_PRIMITIVE_BOOL);
    assert_eq!(t_false.payload, vec![0]);
    assert!(!t_false.as_bool().unwrap());

    let t_invalid = Tin::new(LABEL_PRIMITIVE_BOOL, vec![2]);
    assert_eq!(t_invalid.as_bool().unwrap_err(), TinError::InvalidBool(2));
}

#[test]
// spec: tin-rs/label alhalte unix-time
fn test_unix_time() {
    let ts = 1776412800u64;
    let tin = Tin::from_unix_time(ts);
    assert_eq!(tin.label, LABEL_ALHALTE_UNIX_TIME);
    assert_eq!(tin.as_unix_time().unwrap(), ts);
}

#[test]
// spec: tin-rs/label alhalte tin-id
fn test_tin_id_primitive() {
    let id: TinId = [0x77u8; 32];
    let tin = Tin::from_tin_id(id);
    assert_eq!(tin.label, LABEL_ALHALTE_TIN_ID);
    assert_eq!(tin.as_tin_id().unwrap(), id);
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct TestStruct {
    id: u64,
    name: String,
    items: Vec<i32>,
}

#[test]
// spec: tin-rs/structured postcard
fn test_structured_contents_postcard() {
    let val = TestStruct {
        id: 100,
        name: "Banquette Room".to_string(),
        items: vec![1, -2, 3],
    };

    let tin = Tin::from_postcard("alhalte/test-struct", &val).unwrap();
    assert_eq!(tin.label, label_hash("alhalte/test-struct"));

    let decoded: TestStruct = tin.to_postcard().unwrap();
    assert_eq!(decoded, val);
}

#[test]
// spec: tin-rs/raw encode and decode
fn test_raw_plaintext_bytes_round_trip() {
    // Testing arbitrary raw plaintext (e.g., PADMÉ padded bytes)
    let mut raw_plaintext = Vec::new();
    raw_plaintext.extend_from_slice(&label_hash("custom/padme").to_le_bytes());
    raw_plaintext.extend_from_slice(b"Some secret message that will be padded");
    raw_plaintext.push(0x80); // ISO/IEC 7816-4 padding delimiter
    raw_plaintext.resize(128, 0x00); // Zero padded to 128 bytes

    let room_key = [0xAAu8; 32];
    let nonce = [0x11u8; 12];

    // Encode raw room key
    let encoded = encode_raw_plaintext_bytes(
        &raw_plaintext,
        Compression::Zstd,
        EncryptionParam::RoomKey {
            room_key: &room_key,
            nonce: &nonce,
        },
    )
    .unwrap();

    // Decode raw
    let (decoded_raw, opener, tin_id) =
        decode_raw_plaintext_bytes(&encoded.wire_bytes, Some(&room_key)).unwrap();

    assert_eq!(decoded_raw, raw_plaintext);
    assert_eq!(opener.encryption, Encryption::RoomKey);
    assert_eq!(opener.compression, Compression::Zstd);
    assert_eq!(tin_id, encoded.tin_id);
}

#[test]
// spec: tin-rs/wire format encode and decode
fn test_all_wire_combinations() {
    let test_data = "Comprehensive matrix test across all encryption and compression modes!";
    let tin = Tin::from_string(test_data);
    let room_key = [0x33u8; 32];
    let nonce = [9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 1, 2];

    for comp in [
        Compression::None,
        Compression::Zstd,
        Compression::Lz4,
        Compression::Brotli,
    ] {
        // 1. Plaintext
        let (wire_plain, id_plain) = tin.encode_plaintext(comp).unwrap();
        let (dec_plain, op_plain, dec_id_plain) = decode(&wire_plain, None).unwrap();
        assert_eq!(dec_plain, tin);
        assert_eq!(op_plain.encryption, Encryption::Plaintext);
        assert_eq!(op_plain.compression, comp);
        assert_eq!(dec_id_plain, id_plain);

        // 2. Convergent
        let (wire_conv, id_conv, key_conv) = tin.encode_convergent(comp).unwrap();
        let (dec_conv, op_conv, dec_id_conv) = decode(&wire_conv, Some(&key_conv)).unwrap();
        assert_eq!(dec_conv, tin);
        assert_eq!(op_conv.encryption, Encryption::Convergent);
        assert_eq!(op_conv.compression, comp);
        assert_eq!(dec_id_conv, id_conv);

        // 3. RoomKey
        let (wire_room, id_room) = tin.encode_room_key(comp, &room_key, &nonce).unwrap();
        let (dec_room, op_room, dec_id_room) = decode(&wire_room, Some(&room_key)).unwrap();
        assert_eq!(dec_room, tin);
        assert_eq!(op_room.encryption, Encryption::RoomKey);
        assert_eq!(op_room.compression, comp);
        assert_eq!(dec_id_room, id_room);
    }
}
