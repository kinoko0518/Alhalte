//! Encryption and decryption integration using `rops` and `age`.

use crate::config::BistrotConfig;
use crate::error::{BistrotError, Result};
use crate::ffi::with_scoped_env_var;
use rops::cryptography::cipher::AES256GCM;
use rops::cryptography::hasher::SHA512;
use rops::file::RopsFile;
use rops::file::builder::RopsFileBuilder;
use rops::file::format::TomlFileFormat;
use rops::file::state::EncryptedFile;
use rops::integration::AgeIntegration;
use std::path::Path;
use std::str::FromStr;

/// Checks if the content string appears to be encrypted with rops / sops.
pub fn is_rops_encrypted(content: &str) -> bool {
    content.contains("[sops]") || content.contains("ENC[")
}

/// Encrypts a plain TOML string using the provided age recipient public key (e.g. `age1...`).
///
/// # Errors
///
/// Returns [`BistrotError::RopsEncrypt`] if parsing the recipient or encryption fails.
pub fn encrypt_bistrot_toml(plain_toml: &str, recipient_str: &str) -> Result<String> {
    let recipient: age::x25519::Recipient = recipient_str
        .trim()
        .parse()
        .map_err(|e: &str| BistrotError::RopsEncrypt(format!("invalid age recipient: {e}")))?;

    let builder = RopsFileBuilder::<TomlFileFormat>::new(plain_toml)
        .map_err(|e| BistrotError::RopsEncrypt(format!("failed to parse plain TOML: {e}")))?;

    let encrypted_file = builder
        .add_integration_key::<AgeIntegration>(recipient)
        .encrypt::<AES256GCM, SHA512>()
        .map_err(|e| BistrotError::RopsEncrypt(format!("failed to encrypt with rops: {e}")))?;

    Ok(encrypted_file.to_string())
}

/// Decrypts a rops-encrypted TOML string using available credentials (such as `ROPS_AGE` env var).
///
/// # Errors
///
/// Returns [`BistrotError::RopsDecrypt`] if decryption fails or MAC does not match.
pub fn decrypt_rops_toml(encrypted_toml: &str) -> Result<String> {
    let parsed_encrypted =
        RopsFile::<EncryptedFile<AES256GCM, SHA512>, TomlFileFormat>::from_str(encrypted_toml)
            .map_err(|e| BistrotError::RopsDecrypt(format!("failed to parse rops file: {e}")))?;

    let decrypted = parsed_encrypted
        .decrypt::<TomlFileFormat>()
        .map_err(|e| BistrotError::RopsDecrypt(format!("failed to decrypt rops file: {e}")))?;

    let map = decrypted.into_inner_map();
    let table_val = toml::Value::Table(map);
    toml::to_string_pretty(&table_val).map_err(Into::into)
}

/// Decrypts a rops-encrypted TOML string with an explicitly specified age secret key.
///
/// # Errors
///
/// Returns [`BistrotError::RopsDecrypt`] if decryption fails.
pub fn decrypt_rops_toml_with_key(encrypted_toml: &str, age_key: &str) -> Result<String> {
    with_scoped_env_var("ROPS_AGE", age_key.trim(), || {
        decrypt_rops_toml(encrypted_toml)
    })
}

/// Parses a `Bistrot.toml` configuration string.
///
/// If the string contains `rops` encryption metadata, it will attempt to decrypt it
/// using the environment's `ROPS_AGE` key or default key file.
/// If it is plain TOML, it parses directly.
///
/// # Errors
///
/// Returns [`BistrotError`] if decryption, parsing, or validation fails.
pub fn parse_bistrot_toml(content: &str) -> Result<BistrotConfig> {
    let config: BistrotConfig = if is_rops_encrypted(content) {
        let decrypted_toml = decrypt_rops_toml(content)?;
        toml::from_str(&decrypted_toml)?
    } else {
        toml::from_str(content)?
    };

    config.validate()?;
    Ok(config)
}

/// Parses a `Bistrot.toml` configuration string with an optional explicit age secret key.
///
/// # Errors
///
/// Returns [`BistrotError`] if decryption, parsing, or validation fails.
pub fn parse_bistrot_toml_with_key(content: &str, age_key: Option<&str>) -> Result<BistrotConfig> {
    let config: BistrotConfig = match (is_rops_encrypted(content), age_key) {
        (true, Some(key)) => {
            let decrypted_toml = decrypt_rops_toml_with_key(content, key)?;
            toml::from_str(&decrypted_toml)?
        }
        (true, None) => {
            let decrypted_toml = decrypt_rops_toml(content)?;
            toml::from_str(&decrypted_toml)?
        }
        (false, _) => toml::from_str(content)?,
    };

    config.validate()?;
    Ok(config)
}

/// Loads and parses a `Bistrot.toml` configuration from the specified file path.
///
/// # Errors
///
/// Returns [`BistrotError`] if reading the file, decrypting, or parsing fails.
pub fn load_from_path(path: impl AsRef<Path>) -> Result<BistrotConfig> {
    let content = std::fs::read_to_string(path)?;
    parse_bistrot_toml(&content)
}

/// Loads and parses a `Bistrot.toml` configuration from the specified file path with an optional age key.
///
/// # Errors
///
/// Returns [`BistrotError`] if reading the file, decrypting, or parsing fails.
pub fn load_from_path_with_key(
    path: impl AsRef<Path>,
    age_key: Option<&str>,
) -> Result<BistrotConfig> {
    let content = std::fs::read_to_string(path)?;
    parse_bistrot_toml_with_key(&content, age_key)
}
