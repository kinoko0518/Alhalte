//! Tests for PADMÉ padding and ISO/IEC 7816-4 delimiter padding.

use alhalte_canner::{pad_iso7816_4_padme, padme_padded_len, unpad_iso7816_4_padme};

// spec: alhalte-canner/padme
#[test]
fn test_padme_padded_len_edge_cases() {
    assert_eq!(padme_padded_len(0), 0);
    assert_eq!(padme_padded_len(1), 1);
    assert_eq!(padme_padded_len(2), 2);
    assert_eq!(padme_padded_len(3), 3);
    assert_eq!(padme_padded_len(4), 4);
    assert_eq!(padme_padded_len(7), 7);
    assert_eq!(padme_padded_len(8), 8);
    assert_eq!(padme_padded_len(9), 10);
    assert_eq!(padme_padded_len(10), 10);
    assert_eq!(padme_padded_len(11), 12);
    assert_eq!(padme_padded_len(12), 12);
    assert_eq!(padme_padded_len(16), 16);
    assert_eq!(padme_padded_len(17), 18);
    assert_eq!(padme_padded_len(32), 32);
    assert_eq!(padme_padded_len(33), 36);
    assert_eq!(padme_padded_len(64), 64);
    assert_eq!(padme_padded_len(65), 72);
    assert_eq!(padme_padded_len(128), 128);
    assert_eq!(padme_padded_len(129), 144);
    assert_eq!(padme_padded_len(256), 256);
    assert_eq!(padme_padded_len(257), 272);
}

// spec: alhalte-canner/padme
#[test]
fn test_padme_monotonicity_and_overhead() {
    // For every length up to 10,000 bytes:
    // 1. padded_len >= original_len
    // 2. overhead <= 12% (11.11...%)
    for len in 1..=10_000 {
        let padded = padme_padded_len(len);
        assert!(padded >= len, "padded ({padded}) must be >= len ({len})");
        let overhead = (padded - len) as f64 / len as f64;
        assert!(
            overhead <= 0.125,
            "overhead {overhead} exceeded 12% for len {len}"
        );
    }
}

// spec: alhalte-canner/encrypt-low-entropy
#[test]
fn test_iso7816_4_padme_roundtrip() {
    let cases: &[&[u8]] = &[
        b"",
        b"a",
        b"hello",
        b"1234567",
        b"12345678",
        b"123456789",
        b"Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
    ];

    for &plaintext in cases {
        let padded = pad_iso7816_4_padme(plaintext);
        assert!(padded.len() > plaintext.len());
        assert_eq!(padded.len(), padme_padded_len(plaintext.len() + 1));

        let unpadded = unpad_iso7816_4_padme(&padded).expect("unpad should succeed");
        assert_eq!(unpadded, plaintext);
    }
}

// spec: alhalte-canner/encrypt-low-entropy
#[test]
fn test_iso7816_4_invalid_padding() {
    // Empty padded input
    assert!(unpad_iso7816_4_padme(&[]).is_err());

    // All zero bytes (no 0x80 delimiter)
    assert!(unpad_iso7816_4_padme(&[0x00, 0x00, 0x00]).is_err());

    // Corrupted last non-zero byte
    assert!(unpad_iso7816_4_padme(&[0x12, 0x34, 0x7f, 0x00]).is_err());
    assert!(unpad_iso7816_4_padme(&[0x12, 0x34, 0x81]).is_err());
}
