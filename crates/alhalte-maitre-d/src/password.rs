//! Local account password hashing and validation.
//!
//! Passwords are stored as Argon2id PHC strings (`$argon2id$...`), which include
//! a per-password random salt. The plaintext password and the stored hash are
//! never logged, serialized to a client, or included in an error message.
//!
//! The local login verifies a password outside the database lock. The database
//! module only stores and retrieves the PHC string.

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::error::{MaitreDError, Result};

/// Minimum accepted local-login password length, in Unicode scalar values.
pub const MIN_PASSWORD_LEN: usize = 8;

/// Maximum accepted local-login password length, in Unicode scalar values.
pub const MAX_PASSWORD_LEN: usize = 128;

/// Validates a local-login password.
///
/// The password is **not** trimmed: leading and trailing spaces are part of the
/// credential. Control characters are rejected because they cannot be entered
/// reliably and are usually a sign of an accidental paste.
///
/// # Errors
///
/// Returns [`MaitreDError::BadRequest`] with a user-facing explanation when the
/// password is too short, too long, or contains a control character.
pub fn validate_password(raw: &str) -> Result<()> {
    let length = raw.chars().count();
    if length < MIN_PASSWORD_LEN {
        return Err(MaitreDError::BadRequest(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    if length > MAX_PASSWORD_LEN {
        return Err(MaitreDError::BadRequest(format!(
            "password must be at most {MAX_PASSWORD_LEN} characters"
        )));
    }
    if raw.chars().any(char::is_control) {
        return Err(MaitreDError::BadRequest(
            "password must not contain control characters".to_string(),
        ));
    }
    Ok(())
}

/// Hashes a password with Argon2id and a fresh random salt.
///
/// Returns the PHC string (`$argon2id$v=19$m=...$...`) to store in
/// `accounts.password_hash`.
///
/// # Errors
///
/// Returns [`MaitreDError::PasswordHash`] when Argon2 fails. The error message
/// never includes the password or the generated hash.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| MaitreDError::PasswordHash(error.to_string()))
}

/// Verifies `password` against a stored Argon2 PHC string.
///
/// Returns `Ok(false)` for a wrong password and `Err` when the stored value is
/// not a valid password hash (a data-corruption error, not an auth failure).
///
/// # Errors
///
/// Returns [`MaitreDError::PasswordHash`] when the stored PHC string cannot be
/// parsed.
pub fn verify_password(password: &str, stored: &str) -> Result<bool> {
    let parsed =
        PasswordHash::new(stored).map_err(|error| MaitreDError::PasswordHash(error.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: alhalte-maitre-d/local-login 0004 passwords are stored as salted Argon2id hashes
    #[test]
    fn hashes_and_verifies_a_password() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert_ne!(hash, "correct horse battery staple");
        assert!(verify_password("correct horse battery staple", &hash).unwrap());
        assert!(!verify_password("wrong password", &hash).unwrap());
    }

    #[test]
    fn the_same_password_gets_distinct_salts() {
        let first = hash_password("same password").unwrap();
        let second = hash_password("same password").unwrap();
        assert_ne!(first, second, "each hash must use a fresh random salt");
        assert!(verify_password("same password", &first).unwrap());
        assert!(verify_password("same password", &second).unwrap());
    }

    #[test]
    fn rejects_a_malformed_stored_hash() {
        assert!(matches!(
            verify_password("secret", "not-a-phc-string"),
            Err(MaitreDError::PasswordHash(_))
        ));
    }

    // spec: alhalte-maitre-d/local-login 0005 password length and control characters are validated
    #[test]
    fn validates_length_and_control_characters() {
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("1234567").is_err());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_LEN)).is_ok());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_LEN + 1)).is_err());
        assert!(validate_password("bad\npassword").is_err());
        assert!(
            validate_password("  spaced  ").is_ok(),
            "spaces are part of the credential"
        );
    }
}
