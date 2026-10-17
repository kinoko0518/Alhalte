//! PADMÉ padding and ISO/IEC 7816-4 padding functions.
//!
//! Implements size-obfuscating padding for low-entropy payloads according to
//! the PADMÉ algorithm (PURBs, PoPETs 2019) combined with ISO/IEC 7816-4 delimiter padding.

use crate::error::{CannerError, Result};

/// Calculates the target padded length for `len` using the PADMÉ algorithm.
///
/// PADMÉ computes floating-point-like bounds:
/// `E = floor(log2(L))`
/// `S = floor(log2(E)) + 1`
/// rounding up to multiples of `2^(E - S)`.
///
/// This provides $\mathcal{O}(\log \log M)$ length leakage while bounding overhead to at most ~11.1%.
///
/// // spec: alhalte-canner/padme
pub fn padme_padded_len(len: usize) -> usize {
    if len <= 1 {
        return len;
    }
    let len_u64 = len as u64;
    let e = 63 - len_u64.leading_zeros() as usize;
    let s = 64 - (e as u64).leading_zeros() as usize;
    let z = e.saturating_sub(s);
    let mask = (1usize << z) - 1;
    (len + mask) & !mask
}

/// Applies ISO/IEC 7816-4 padding to `plaintext` up to the PADMÉ padded length.
///
/// Appends `0x80`, followed by `0x00` bytes until the total length reaches `padme_padded_len(plaintext.len() + 1)`.
///
/// // spec: alhalte-canner/encrypt-low-entropy
pub fn pad_iso7816_4_padme(plaintext: &[u8]) -> Vec<u8> {
    let min_len = plaintext.len() + 1;
    let target_len = padme_padded_len(min_len);
    let mut padded = Vec::with_capacity(target_len);
    padded.extend_from_slice(plaintext);
    padded.push(0x80);
    padded.resize(target_len, 0x00);
    padded
}

/// Removes ISO/IEC 7816-4 padding from `padded` to recover the original plaintext slice.
///
/// Finds the last non-zero byte from the end and verifies that it is `0x80`.
/// Returns an error if the padding is invalid or absent.
///
/// // spec: alhalte-canner/encrypt-low-entropy
pub fn unpad_iso7816_4_padme(padded: &[u8]) -> Result<&[u8]> {
    if padded.is_empty() {
        return Err(CannerError::InvalidPadding);
    }
    let last_non_zero = padded.iter().rposition(|&b| b != 0x00);
    match last_non_zero {
        Some(idx) if padded[idx] == 0x80 => Ok(&padded[..idx]),
        _ => Err(CannerError::InvalidPadding),
    }
}
