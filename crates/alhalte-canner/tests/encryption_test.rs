//! Tests for low-entropy and high-entropy encryption in `alhalte-canner`.

use alhalte_canner::{
    decrypt_high_entropy, decrypt_low_entropy, encrypt_high_entropy, encrypt_high_entropy_random,
    encrypt_low_entropy, encrypt_low_entropy_random,
};
use tin::{Compression, Encryption, Tin};

// spec: alhalte-canner/encrypt-low-entropy
#[test]
fn test_encrypt_decrypt_low_entropy_roundtrip() {
    let room_key = [0x42u8; 32];
    let nonce = [0x07u8; 12];
    let original_tin = Tin::from_string("Hello, low entropy chat message!");

    for compression in [
        Compression::None,
        Compression::Zstd,
        Compression::Lz4,
        Compression::Brotli,
    ] {
        let (wire_bytes, tin_id) =
            encrypt_low_entropy(&original_tin, compression, &room_key, &nonce)
                .expect("encryption should succeed");

        // Verify wire format: 1 byte opener + 12 bytes nonce + body
        assert!(wire_bytes.len() >= 13);
        assert_eq!(&wire_bytes[1..13], &nonce);

        // Verify decryption and unpadding
        let (decrypted_tin, opener, decrypted_id) =
            decrypt_low_entropy(&wire_bytes, &room_key).expect("decryption should succeed");

        assert_eq!(opener.encryption, Encryption::RoomKey);
        assert_eq!(opener.compression, compression);
        assert_eq!(tin_id, decrypted_id);
        assert_eq!(decrypted_tin.label, original_tin.label);
        assert_eq!(decrypted_tin.payload, original_tin.payload);
        assert_eq!(
            decrypted_tin.as_string().unwrap(),
            "Hello, low entropy chat message!"
        );
    }
}

// spec: alhalte-canner/encrypt-low-entropy
#[test]
fn test_encrypt_low_entropy_random() {
    let room_key = [0x55u8; 32];
    let original_tin = Tin::from_u64(123456789);

    let (wire1, id1) = encrypt_low_entropy_random(&original_tin, Compression::None, &room_key)
        .expect("encryption 1 should succeed");
    let (wire2, id2) = encrypt_low_entropy_random(&original_tin, Compression::None, &room_key)
        .expect("encryption 2 should succeed");

    // Random nonces should produce different wire bytes and Tin IDs (since TinId is hash of wire_bytes)
    assert_ne!(wire1, wire2);
    assert_ne!(id1, id2);

    let (dec1, _, _) = decrypt_low_entropy(&wire1, &room_key).unwrap();
    let (dec2, _, _) = decrypt_low_entropy(&wire2, &room_key).unwrap();
    assert_eq!(dec1.as_u64().unwrap(), 123456789);
    assert_eq!(dec2.as_u64().unwrap(), 123456789);
}

// spec: alhalte-canner/encrypt-low-entropy
#[test]
fn test_decrypt_low_entropy_failures() {
    let room_key = [0x42u8; 32];
    let wrong_key = [0x99u8; 32];
    let nonce = [0x01u8; 12];
    let original_tin = Tin::from_string("Test failure");

    let (wire_bytes, _) =
        encrypt_low_entropy(&original_tin, Compression::None, &room_key, &nonce).unwrap();

    // Wrong room key fails
    assert!(decrypt_low_entropy(&wire_bytes, &wrong_key).is_err());

    // Corrupted wire bytes fails
    let mut corrupted = wire_bytes.clone();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 0xff;
    assert!(decrypt_low_entropy(&corrupted, &room_key).is_err());
}

// spec: alhalte-canner/encrypt-high-entropy
#[test]
fn test_encrypt_decrypt_high_entropy_roundtrip() {
    let room_key = [0xaau8; 32];
    let wrap_nonce = [0x11u8; 12];
    let payload = vec![0xa5u8; 1000]; // high-entropy-like payload
    let original_tin = Tin::new(0x1234, payload);

    for compression in [
        Compression::None,
        Compression::Zstd,
        Compression::Lz4,
        Compression::Brotli,
    ] {
        let output = encrypt_high_entropy(&original_tin, compression, &room_key, &wrap_nonce)
            .expect("encryption should succeed");

        // Wrapped key structure: 12B nonce + 32B key + 16B GCM tag = 60 bytes
        assert_eq!(output.wrapped_key.len(), 60);
        assert_eq!(&output.wrapped_key[..12], &wrap_nonce);

        // Tin bytes has opener with convergent encryption (0001)
        assert_eq!(output.tin_bytes[0] >> 4, 0x01);

        let (decrypted_tin, opener, decrypted_id) =
            decrypt_high_entropy(&output.tin_bytes, &output.wrapped_key, &room_key)
                .expect("decryption should succeed");

        assert_eq!(opener.encryption, Encryption::Convergent);
        assert_eq!(opener.compression, compression);
        assert_eq!(output.tin_id, decrypted_id);
        assert_eq!(decrypted_tin, original_tin);
    }
}

// spec: alhalte-canner/encrypt-high-entropy
#[test]
fn test_encrypt_high_entropy_convergent_deduplication() {
    let room_key1 = [0x01u8; 32];
    let room_key2 = [0x02u8; 32];
    let wrap_nonce1 = [0xaa; 12];
    let wrap_nonce2 = [0xbb; 12];
    let original_tin = Tin::from_string("Identical high entropy asset data");

    // Two encryptions of identical content with different room keys / wrap nonces
    let out1 =
        encrypt_high_entropy(&original_tin, Compression::Zstd, &room_key1, &wrap_nonce1).unwrap();
    let out2 =
        encrypt_high_entropy(&original_tin, Compression::Zstd, &room_key2, &wrap_nonce2).unwrap();

    // Convergent encryption ensures identical tin_id and tin_bytes (zero nonce, deterministic key)
    assert_eq!(out1.tin_id, out2.tin_id);
    assert_eq!(out1.tin_bytes, out2.tin_bytes);

    // Wrapped keys differ because room keys and wrap nonces differ
    assert_ne!(out1.wrapped_key, out2.wrapped_key);

    // Both can be decrypted with their respective room keys
    let (dec1, _, id1) =
        decrypt_high_entropy(&out1.tin_bytes, &out1.wrapped_key, &room_key1).unwrap();
    let (dec2, _, id2) =
        decrypt_high_entropy(&out2.tin_bytes, &out2.wrapped_key, &room_key2).unwrap();
    assert_eq!(dec1, original_tin);
    assert_eq!(dec2, original_tin);
    assert_eq!(id1, out1.tin_id);
    assert_eq!(id2, out2.tin_id);
}

// spec: alhalte-canner/encrypt-high-entropy
#[test]
fn test_encrypt_high_entropy_random() {
    let room_key = [0x77u8; 32];
    let original_tin = Tin::from_string("Random wrap nonce test");

    let out1 = encrypt_high_entropy_random(&original_tin, Compression::Lz4, &room_key).unwrap();
    let out2 = encrypt_high_entropy_random(&original_tin, Compression::Lz4, &room_key).unwrap();

    // Same content -> same convergent tin_id and tin_bytes
    assert_eq!(out1.tin_id, out2.tin_id);
    assert_eq!(out1.tin_bytes, out2.tin_bytes);

    // Random wrap nonces -> different wrapped keys
    assert_ne!(out1.wrapped_key, out2.wrapped_key);

    let (dec1, _, _) = decrypt_high_entropy(&out1.tin_bytes, &out1.wrapped_key, &room_key).unwrap();
    let (dec2, _, _) = decrypt_high_entropy(&out2.tin_bytes, &out2.wrapped_key, &room_key).unwrap();
    assert_eq!(dec1, original_tin);
    assert_eq!(dec2, original_tin);
}

// spec: alhalte-canner/encrypt-high-entropy
#[test]
fn test_decrypt_high_entropy_failures() {
    let room_key = [0x11u8; 32];
    let wrong_key = [0x22u8; 32];
    let wrap_nonce = [0x33u8; 12];
    let original_tin = Tin::from_string("Fail test");

    let out =
        encrypt_high_entropy(&original_tin, Compression::None, &room_key, &wrap_nonce).unwrap();

    // Wrong room key fails
    assert!(decrypt_high_entropy(&out.tin_bytes, &out.wrapped_key, &wrong_key).is_err());

    // Corrupted wrapped key fails
    let mut bad_wrapped = out.wrapped_key.clone();
    let len = bad_wrapped.len();
    bad_wrapped[len - 1] ^= 0x01;
    assert!(decrypt_high_entropy(&out.tin_bytes, &bad_wrapped, &room_key).is_err());

    // Truncated wrapped key fails
    assert!(decrypt_high_entropy(&out.tin_bytes, &out.wrapped_key[..20], &room_key).is_err());
}
