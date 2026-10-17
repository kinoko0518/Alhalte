//! Format identifiers, 64-bit labels, and `Tin` conversion helpers for `alhalte-maitre-d`.

use tin::{Label, Tin};

use crate::error::{MaitreDError, Result};

/// Format identifier for account ID: `alhalte/maitre-d/account-id`.
pub const IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID: &str = "alhalte/maitre-d/account-id";

/// Format identifier for session ID: `alhalte/maitre-d/session-id`.
pub const IDENT_ALHALTE_MAITRE_D_SESSION_ID: &str = "alhalte/maitre-d/session-id";

/// Precomputed 64-bit XXH64 label for `alhalte/maitre-d/account-id`.
pub const LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID.as_bytes(), 0);

/// Precomputed 64-bit XXH64 label for `alhalte/maitre-d/session-id`.
pub const LABEL_ALHALTE_MAITRE_D_SESSION_ID: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_MAITRE_D_SESSION_ID.as_bytes(), 0);

/// Encodes a 64-bit account ID into a `Tin` container with label `alhalte/maitre-d/account-id`.
#[must_use]
pub fn account_id_to_tin(account_id: u64) -> Tin {
    Tin::new(
        LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID,
        account_id.to_le_bytes().to_vec(),
    )
}

/// Decodes a 64-bit account ID from a `Tin` container.
///
/// # Errors
///
/// Returns [`MaitreDError::InvalidTin`] if the label is not `alhalte/maitre-d/account-id`
/// or if the payload is not exactly 8 bytes.
pub fn account_id_from_tin(tin: &Tin) -> Result<u64> {
    if tin.label != LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID {
        return Err(MaitreDError::InvalidTin(format!(
            "Expected account-id label 0x{:016x}, got 0x{:016x}",
            LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID, tin.label
        )));
    }
    if tin.payload.len() != 8 {
        return Err(MaitreDError::InvalidTin(format!(
            "Expected 8 bytes for account-id, got {} bytes",
            tin.payload.len()
        )));
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&tin.payload);
    Ok(u64::from_le_bytes(bytes))
}

/// Encodes a 64-bit session ID into a `Tin` container with label `alhalte/maitre-d/session-id`.
#[must_use]
pub fn session_id_to_tin(session_id: u64) -> Tin {
    Tin::new(
        LABEL_ALHALTE_MAITRE_D_SESSION_ID,
        session_id.to_le_bytes().to_vec(),
    )
}

/// Decodes a 64-bit session ID from a `Tin` container.
///
/// # Errors
///
/// Returns [`MaitreDError::InvalidTin`] if the label is not `alhalte/maitre-d/session-id`
/// or if the payload is not exactly 8 bytes.
pub fn session_id_from_tin(tin: &Tin) -> Result<u64> {
    if tin.label != LABEL_ALHALTE_MAITRE_D_SESSION_ID {
        return Err(MaitreDError::InvalidTin(format!(
            "Expected session-id label 0x{:016x}, got 0x{:016x}",
            LABEL_ALHALTE_MAITRE_D_SESSION_ID, tin.label
        )));
    }
    if tin.payload.len() != 8 {
        return Err(MaitreDError::InvalidTin(format!(
            "Expected 8 bytes for session-id, got {} bytes",
            tin.payload.len()
        )));
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&tin.payload);
    Ok(u64::from_le_bytes(bytes))
}
