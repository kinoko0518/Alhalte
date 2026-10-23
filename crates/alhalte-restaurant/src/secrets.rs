//! Value-level `age` encryption for `Restaurant.toml`, and secret file
//! materialization.
//!
//! Issue #194 makes per-value encryption the first-class secret format: the
//! configuration stays a plain TOML document and only the string values that are
//! secrets are replaced with an ASCII-armored `age` ciphertext:
//!
//! ```toml
//! [cellar]
//! service-token = """
//! -----BEGIN AGE ENCRYPTED FILE-----
//! ...
//! -----END AGE ENCRYPTED FILE-----
//! """
//! ```
//!
//! [`crate::rops::parse_restaurant_toml_with_options`] walks every TOML value,
//! decrypts the armored strings, and only then deserializes the configuration.
//! Decrypted secrets are never put on a command line: [`write_secret_files`]
//! writes them to `<data-dir>/secrets/` with mode `0600` and the argument
//! generators pass the file paths instead of the values.

use crate::config::RestaurantToml;
use crate::error::{RestaurantError, Result};
use age::x25519::{Identity, Recipient};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Environment variable holding an age identity (lowest-precedence source).
pub const AGE_KEY_ENV: &str = "ALHALTE_AGE_KEY";
/// Environment variable holding an age identity (used by `rops` itself).
pub const ROPS_AGE_ENV: &str = "ROPS_AGE";
/// Development-only escape hatch that accepts plaintext secrets.
pub const ALLOW_PLAINTEXT_SECRETS_ENV: &str = "ALHALTE_ALLOW_PLAINTEXT_SECRETS";
/// First line of an ASCII-armored `age` ciphertext.
pub const AGE_ARMOR_BEGIN: &str = "-----BEGIN AGE ENCRYPTED FILE-----";
/// Last line of an ASCII-armored `age` ciphertext.
pub const AGE_ARMOR_END: &str = "-----END AGE ENCRYPTED FILE-----";

/// Returns true when `value` is (or starts with) an ASCII-armored `age`
/// ciphertext.
pub fn is_age_armored_value(value: &str) -> bool {
    value.trim_start().starts_with(AGE_ARMOR_BEGIN)
}

/// Reads `ALHALTE_ALLOW_PLAINTEXT_SECRETS` and reports whether plaintext secrets
/// are explicitly allowed (development only).
///
/// Only the exact value `1` enables the escape hatch; anything else keeps the
/// fail-closed behavior.
pub fn allow_plaintext_secrets_from_env() -> bool {
    std::env::var(ALLOW_PLAINTEXT_SECRETS_ENV).is_ok_and(|value| value.trim() == "1")
}

/// Parses an `age` x25519 identity (e.g. `AGE-SECRET-KEY-1...`).
///
/// # Errors
///
/// Returns [`RestaurantError::AgeIdentity`] when the string is not a valid
/// x25519 identity.
pub fn parse_age_identity(key: &str) -> Result<Identity> {
    key.trim()
        .parse::<Identity>()
        .map_err(|e: &str| RestaurantError::AgeIdentity(e.to_string()))
}

/// Parses an `age` x25519 recipient (e.g. `age1...`).
///
/// # Errors
///
/// Returns [`RestaurantError::AgeEncrypt`] when the string is not a valid
/// x25519 recipient.
pub fn parse_age_recipient(recipient: &str) -> Result<Recipient> {
    recipient
        .trim()
        .parse::<Recipient>()
        .map_err(|e| RestaurantError::AgeEncrypt(format!("invalid age recipient: {e}")))
}

/// Resolves the age identity source in the documented precedence order:
/// explicit argument (`--age-key`) → `ROPS_AGE` → `ALHALTE_AGE_KEY`.
///
/// Returns `None` when no source is configured.
pub fn resolve_age_identity_source(explicit: Option<&str>) -> Option<String> {
    let non_empty = |value: String| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    };

    if let Some(key) = explicit.and_then(|key| non_empty(key.to_string())) {
        return Some(key);
    }
    for name in [ROPS_AGE_ENV, AGE_KEY_ENV] {
        if let Ok(value) = std::env::var(name)
            && let Some(key) = non_empty(value)
        {
            return Some(key);
        }
    }
    None
}

/// Decrypts an ASCII-armored `age` value with `identity`.
///
/// # Errors
///
/// Returns [`RestaurantError::AgeDecrypt`] when the armor is malformed, the
/// identity does not match, or the plaintext is not valid UTF-8.
pub fn decrypt_age_value(armored: &str, identity: &Identity) -> Result<String> {
    let plaintext = age::decrypt(identity, armored.trim().as_bytes())
        .map_err(|e| RestaurantError::AgeDecrypt(e.to_string()))?;
    String::from_utf8(plaintext)
        .map_err(|e| RestaurantError::AgeDecrypt(format!("decrypted value is not UTF-8: {e}")))
}

/// Encrypts `plaintext` to `recipient` and returns the ASCII-armored ciphertext.
///
/// # Errors
///
/// Returns [`RestaurantError::AgeEncrypt`] when `age` fails to encrypt.
pub fn encrypt_age_value(plaintext: &str, recipient: &Recipient) -> Result<String> {
    age::encrypt_and_armor(recipient, plaintext.as_bytes())
        .map_err(|e| RestaurantError::AgeEncrypt(e.to_string()))
}

/// Returns true when any string value in `value` is an armored age ciphertext.
pub fn contains_age_armored_value(value: &toml::Value) -> bool {
    match value {
        toml::Value::String(text) => is_age_armored_value(text),
        toml::Value::Table(table) => table.values().any(contains_age_armored_value),
        toml::Value::Array(items) => items.iter().any(contains_age_armored_value),
        _ => false,
    }
}

/// Recursively decrypts every armored string value in place.
///
/// Returns the number of decrypted values so callers can tell "nothing to do"
/// apart from a successful decryption.
///
/// # Errors
///
/// Returns [`RestaurantError::AgeDecrypt`] when an armored value cannot be
/// decrypted with `identity`.
pub fn decrypt_age_values(value: &mut toml::Value, identity: &Identity) -> Result<usize> {
    match value {
        toml::Value::String(text) => {
            if is_age_armored_value(text) {
                *text = decrypt_age_value(text, identity)?;
                Ok(1)
            } else {
                Ok(0)
            }
        }
        toml::Value::Table(table) => {
            let mut decrypted = 0;
            for value in table.iter_mut().map(|(_, value)| value) {
                decrypted += decrypt_age_values(value, identity)?;
            }
            Ok(decrypted)
        }
        toml::Value::Array(items) => {
            let mut decrypted = 0;
            for value in items.iter_mut() {
                decrypted += decrypt_age_values(value, identity)?;
            }
            Ok(decrypted)
        }
        _ => Ok(0),
    }
}

/// Returns the dotted paths of secret values that are still plaintext.
///
/// Only the fields that are secrets in `Restaurant.toml` are inspected
/// (`maitre-d.client-secret` and `cellar.service-token`), including their
/// snake_case aliases. Empty strings are ignored here: `validate` reports them.
pub fn find_plaintext_secrets(value: &toml::Value) -> Vec<String> {
    let mut found = Vec::new();
    let Some(root) = value.as_table() else {
        return found;
    };

    for section in ["maitre-d", "maitre_d"] {
        let Some(table) = root.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        for key in ["client-secret", "client_secret"] {
            if is_plaintext_secret(table.get(key)) {
                found.push(format!("{section}.{key}"));
            }
        }
    }

    if let Some(table) = root.get("cellar").and_then(toml::Value::as_table) {
        for key in ["service-token", "service_token"] {
            if is_plaintext_secret(table.get(key)) {
                found.push(format!("cellar.{key}"));
            }
        }
    }

    found.sort();
    found.dedup();
    found
}

fn is_plaintext_secret(value: Option<&toml::Value>) -> bool {
    match value.and_then(toml::Value::as_str) {
        Some(text) => {
            let trimmed = text.trim();
            !trimmed.is_empty() && !is_age_armored_value(trimmed)
        }
        None => false,
    }
}

/// Paths of the secret files [`write_secret_files`] materializes.
///
/// The paths are derived deterministically from the configuration so that the
/// argument generators stay pure; they are `None` when the secret is not
/// configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFiles {
    /// Directory holding the materialized secrets (`<data-dir>/secrets`).
    pub dir: PathBuf,
    /// File holding `maitre-d.client-secret`, passed as `--client-secret-file`.
    pub maitre_d_client_secret: Option<PathBuf>,
    /// File holding `cellar.service-token`, passed as `--service-token-file`.
    pub service_token: Option<PathBuf>,
}

/// Computes the secret file paths for `config` without touching the filesystem.
pub fn secret_files(config: &RestaurantToml) -> SecretFiles {
    let dir = config.secrets_dir();
    let maitre_d_client_secret = if config.maitre_d.client_secret.trim().is_empty() {
        None
    } else {
        Some(dir.join("maitre-d-client-secret"))
    };
    let service_token = config
        .cellar
        .service_token
        .as_ref()
        .filter(|token| !token.trim().is_empty())
        .map(|_| dir.join("cellar-service-token"));

    SecretFiles {
        dir,
        maitre_d_client_secret,
        service_token,
    }
}

/// Writes every configured secret to `<data-dir>/secrets/` with mode `0600`.
///
/// The directory itself is created with mode `0700`. Files are created with
/// `0600` from the start (never world-readable) and re-chmodded afterwards
/// because truncating an existing file keeps its old permissions.
///
/// # Errors
///
/// Returns [`RestaurantError::Io`] when the directory or a file cannot be
/// written.
pub fn write_secret_files(config: &RestaurantToml) -> Result<SecretFiles> {
    let files = secret_files(config);
    if files.maitre_d_client_secret.is_none() && files.service_token.is_none() {
        return Ok(files);
    }

    create_private_dir(&files.dir)?;

    if let Some(path) = &files.maitre_d_client_secret {
        write_private_file(path, &config.maitre_d.client_secret)?;
    }
    if let (Some(path), Some(token)) = (&files.service_token, &config.cellar.service_token) {
        write_private_file(path, token)?;
    }

    Ok(files)
}

fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    set_mode(dir, 0o700)?;
    Ok(())
}

fn write_private_file(path: &Path, value: &str) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Create the file as owner-only from the very first byte.
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(value.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);

    set_mode(path, 0o600)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}
