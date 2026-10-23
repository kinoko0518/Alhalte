//! `alhalte-restaurant` — shared `Restaurant.toml` model and argument
//! generation for Alhalte.
//!
//! This crate owns the parts that `alhalte-cafe` (one-shot launcher) and
//! `alhalte-bistrot` (systemd unit generator) must agree on:
//!
//! - Parsing and validating `Restaurant.toml`.
//! - Decrypting per-value `age` armor (first-class) and whole-file rops
//!   (compatibility).
//! - Materializing decrypted secrets as `0600` files under
//!   `<data-dir>/secrets/`, so services receive file paths rather than values.
//! - Generating the `alhalte-maitre-d` / `alhalte-cellar` argument lists as
//!   pure functions.
//! - Staging configured extension wasm files and `manifest.json`.
//!
//! See `crates/alhalte-restaurant/ARCHITECTURE.md`.

#![deny(unsafe_code)]

pub mod args;
pub mod config;
pub mod error;
pub mod ffi;
pub mod rops;
pub mod secrets;

pub use args::{
    MaitreDArgsOptions, ModuleArgs, generate_all_args, generate_cellar_args, generate_maitre_d_args,
};
pub use config::{
    BanquetteConfig, CellarConfig, DEFAULT_BANQUETTE_HEARTBEAT_INTERVAL_SECS, DEFAULT_CELLAR_PORT,
    DEFAULT_CHUNK_SIZE_EXPONENT, DEFAULT_DATA_DIR, DEFAULT_DOMAIN, DEFAULT_HOST,
    DEFAULT_MAITRE_D_PORT, DEFAULT_SOCKET_DIR, DEFAULT_VITE_DEV_PORT, ExtensionConfig,
    ExtensionManifestEntry, MaitreDConfig, RestaurantConfig, RestaurantToml,
};
pub use error::{RestaurantError, Result};
pub use rops::{
    ParseOptions, decrypt_rops_toml, decrypt_rops_toml_with_key, encrypt_age_armored_value,
    encrypt_restaurant_toml, is_rops_encrypted, load_restaurant_from_path,
    load_restaurant_from_path_with_key, load_restaurant_from_path_with_options,
    parse_restaurant_toml, parse_restaurant_toml_with_key, parse_restaurant_toml_with_options,
};
pub use secrets::{
    AGE_ARMOR_BEGIN, AGE_ARMOR_END, ALLOW_PLAINTEXT_SECRETS_ENV, SecretFiles,
    allow_plaintext_secrets_from_env, decrypt_age_value, decrypt_age_values, encrypt_age_value,
    find_plaintext_secrets, is_age_armored_value, parse_age_identity, parse_age_recipient,
    secret_files, write_secret_files,
};
