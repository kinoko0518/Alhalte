//! Encryption and decryption integration using `rops` and `age`.
//!
//! Two secret formats are supported:
//!
//! 1. **Whole-file rops encryption** (SOPS-compatible): the entire TOML document
//!    is encrypted. Detected through `[sops]` / `ENC[` and kept for backward
//!    compatibility.
//! 2. **Per-value age armor** (first-class, Issue #194): the document stays plain
//!    TOML and only secret string values are replaced with an ASCII-armored `age`
//!    ciphertext. See [`crate::secrets`].

use crate::config::RestaurantToml;
use crate::error::{RestaurantError, Result};
use crate::ffi::with_scoped_env_var;
use crate::secrets::{
    allow_plaintext_secrets_from_env, contains_age_armored_value, decrypt_age_values,
    find_plaintext_secrets, parse_age_identity, resolve_age_identity_source,
};
use rops::cryptography::cipher::AES256GCM;
use rops::cryptography::hasher::SHA512;
use rops::file::RopsFile;
use rops::file::builder::RopsFileBuilder;
use rops::file::format::TomlFileFormat;
use rops::file::state::EncryptedFile;
use rops::integration::AgeIntegration;
use serde::Deserialize;
use std::path::Path;
use std::str::FromStr;

/// Options controlling how `Restaurant.toml` is decrypted and validated.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseOptions<'a> {
    /// Explicit age identity (`--age-key`). When `None`, `ROPS_AGE` and
    /// `ALHALTE_AGE_KEY` are consulted in that order.
    pub age_key: Option<&'a str>,
    /// Allows plaintext secrets (development only).
    ///
    /// Defaults to `false`; [`ParseOptions::from_env`] derives it from
    /// `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1`.
    pub allow_plaintext_secrets: bool,
}

impl ParseOptions<'_> {
    /// Builds options from the process environment.
    ///
    /// `allow_plaintext_secrets` follows `ALHALTE_ALLOW_PLAINTEXT_SECRETS`.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            age_key: None,
            allow_plaintext_secrets: allow_plaintext_secrets_from_env(),
        }
    }
}

/// Checks if the content string appears to be encrypted with rops / sops.
pub fn is_rops_encrypted(content: &str) -> bool {
    content.contains("[sops]") || content.contains("ENC[")
}

/// Encrypts a plain TOML string using the provided age recipient public key
/// (e.g. `age1...`).
///
/// # Errors
///
/// Returns [`RestaurantError::RopsEncrypt`] if parsing the recipient or
/// encryption fails.
pub fn encrypt_restaurant_toml(plain_toml: &str, recipient_str: &str) -> Result<String> {
    let recipient: age::x25519::Recipient = recipient_str
        .trim()
        .parse()
        .map_err(|e: &str| RestaurantError::RopsEncrypt(format!("invalid age recipient: {e}")))?;

    let builder = RopsFileBuilder::<TomlFileFormat>::new(plain_toml)
        .map_err(|e| RestaurantError::RopsEncrypt(format!("failed to parse plain TOML: {e}")))?;

    let encrypted_file = builder
        .add_integration_key::<AgeIntegration>(recipient)
        .encrypt::<AES256GCM, SHA512>()
        .map_err(|e| RestaurantError::RopsEncrypt(format!("failed to encrypt with rops: {e}")))?;

    Ok(encrypted_file.to_string())
}

/// Encrypts a single secret value and returns an ASCII-armored `age` block.
///
/// This is the value-level counterpart of [`encrypt_restaurant_toml`]: the
/// result can be pasted into `Restaurant.toml` as the value of
/// `maitre-d.client-secret` or `cellar.service-token` (Issue #194).
///
/// # Errors
///
/// Returns [`RestaurantError::AgeEncrypt`] when the recipient is invalid or
/// `age` fails to encrypt.
pub fn encrypt_age_armored_value(plaintext: &str, recipient_str: &str) -> Result<String> {
    let recipient = crate::secrets::parse_age_recipient(recipient_str)?;
    crate::secrets::encrypt_age_value(plaintext, &recipient)
}

/// Decrypts a rops-encrypted TOML string using available credentials (such as
/// `ROPS_AGE` env var).
///
/// # Errors
///
/// Returns [`RestaurantError::RopsDecrypt`] if decryption fails or MAC does not
/// match.
pub fn decrypt_rops_toml(encrypted_toml: &str) -> Result<String> {
    let parsed_encrypted =
        RopsFile::<EncryptedFile<AES256GCM, SHA512>, TomlFileFormat>::from_str(encrypted_toml)
            .map_err(|e| RestaurantError::RopsDecrypt(format!("failed to parse rops file: {e}")))?;

    let decrypted = parsed_encrypted
        .decrypt::<TomlFileFormat>()
        .map_err(|e| RestaurantError::RopsDecrypt(format!("failed to decrypt rops file: {e}")))?;

    let map = decrypted.into_inner_map();
    let table_val = toml::Value::Table(map);
    toml::to_string_pretty(&table_val).map_err(Into::into)
}

/// Decrypts a rops-encrypted TOML string with an explicitly specified age
/// secret key.
///
/// # Errors
///
/// Returns [`RestaurantError::RopsDecrypt`] if decryption fails.
pub fn decrypt_rops_toml_with_key(encrypted_toml: &str, age_key: &str) -> Result<String> {
    with_scoped_env_var("ROPS_AGE", age_key.trim(), || {
        decrypt_rops_toml(encrypted_toml)
    })
}

/// Parses a `Restaurant.toml` configuration string.
///
/// Handles both formats:
/// - Whole-file `rops` encryption (`[sops]` / `ENC[`) using `ROPS_AGE`.
/// - Per-value age armor (`-----BEGIN AGE ENCRYPTED FILE-----`) using
///   `ROPS_AGE` / `ALHALTE_AGE_KEY`.
///
/// Plaintext secrets are rejected unless `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1`.
///
/// # Errors
///
/// Returns [`RestaurantError`] if decryption, parsing, or validation fails.
pub fn parse_restaurant_toml(content: &str) -> Result<RestaurantToml> {
    parse_restaurant_toml_with_options(content, &ParseOptions::from_env())
}

/// Parses a `Restaurant.toml` configuration string with an optional explicit age
/// secret key.
///
/// The key is used both for whole-file rops decryption and for value-level age
/// armor. When `age_key` is `None`, `ROPS_AGE` and `ALHALTE_AGE_KEY` are
/// consulted. Plaintext-secret handling follows
/// `ALHALTE_ALLOW_PLAINTEXT_SECRETS`.
///
/// # Errors
///
/// Returns [`RestaurantError`] if decryption, parsing, or validation fails.
pub fn parse_restaurant_toml_with_key(
    content: &str,
    age_key: Option<&str>,
) -> Result<RestaurantToml> {
    parse_restaurant_toml_with_options(
        content,
        &ParseOptions {
            age_key,
            ..ParseOptions::from_env()
        },
    )
}

/// Parses a `Restaurant.toml` configuration string with explicit options.
///
/// # Errors
///
/// Returns [`RestaurantError::AgeKeyMissing`] when the document contains armored
/// values and no identity is configured, [`RestaurantError::AgeDecrypt`] when a
/// value cannot be decrypted, [`RestaurantError::PlaintextSecret`] when a secret
/// is still plaintext and plaintext secrets are not allowed, and
/// [`RestaurantError::TomlDe`] / [`RestaurantError::ConfigValidation`] for
/// invalid configuration.
pub fn parse_restaurant_toml_with_options(
    content: &str,
    options: &ParseOptions<'_>,
) -> Result<RestaurantToml> {
    let whole_file_encrypted = is_rops_encrypted(content);
    let identity_source = resolve_age_identity_source(options.age_key);

    let mut value: toml::Value = if whole_file_encrypted {
        let decrypted = match identity_source.as_deref() {
            Some(key) => decrypt_rops_toml_with_key(content, key)?,
            None => decrypt_rops_toml(content)?,
        };
        toml::from_str(&decrypted)?
    } else {
        toml::from_str(content)?
    };

    // Plaintext secrets are detected on the *source* document, before anything
    // is decrypted: once decrypted, every secret is plaintext in memory, which
    // is exactly what the armored form is meant to become.
    let plaintext_secrets = if whole_file_encrypted {
        Vec::new()
    } else {
        find_plaintext_secrets(&value)
    };

    if contains_age_armored_value(&value) {
        let key = identity_source
            .as_deref()
            .ok_or(RestaurantError::AgeKeyMissing)?;
        let identity = parse_age_identity(key)?;
        decrypt_age_values(&mut value, &identity)?;
    }

    // fail-closed: a secret that is still plaintext in an unencrypted document
    // is rejected unless the development escape hatch is explicit. A
    // rops-encrypted whole file is already encrypted at rest, so its decrypted
    // values are fine.
    if !options.allow_plaintext_secrets && !plaintext_secrets.is_empty() {
        return Err(RestaurantError::PlaintextSecret(
            plaintext_secrets.join(", "),
        ));
    }

    let config = RestaurantToml::deserialize(value)?;
    config.validate()?;
    Ok(config)
}

/// Loads and parses a `Restaurant.toml` configuration from the specified file
/// path.
///
/// # Errors
///
/// Returns [`RestaurantError`] if reading the file, decrypting, or parsing
/// fails.
pub fn load_restaurant_from_path(path: impl AsRef<Path>) -> Result<RestaurantToml> {
    let content = std::fs::read_to_string(path)?;
    parse_restaurant_toml(&content)
}

/// Loads and parses a `Restaurant.toml` configuration from the specified file
/// path with an optional age key.
///
/// # Errors
///
/// Returns [`RestaurantError`] if reading the file, decrypting, or parsing
/// fails.
pub fn load_restaurant_from_path_with_key(
    path: impl AsRef<Path>,
    age_key: Option<&str>,
) -> Result<RestaurantToml> {
    let content = std::fs::read_to_string(path)?;
    parse_restaurant_toml_with_key(&content, age_key)
}

/// Loads and parses a `Restaurant.toml` configuration from a file path with
/// explicit options.
///
/// # Errors
///
/// Returns [`RestaurantError`] if reading the file, decrypting, or parsing
/// fails.
pub fn load_restaurant_from_path_with_options(
    path: impl AsRef<Path>,
    options: &ParseOptions<'_>,
) -> Result<RestaurantToml> {
    let content = std::fs::read_to_string(path)?;
    parse_restaurant_toml_with_options(&content, options)
}
