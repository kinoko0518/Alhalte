//! `alhalte-bistrot` — Restaurant configuration manager, argument generator, and systemd generator for Alhalte.
//!
//! # Overview
//!
//! `alhalte-bistrot` serves as the orchestration and configuration manager for an Alhalte restaurant instance:
//! - Parses and validates `Bistrot.toml` configuration files.
//! - Decrypts secrets: per-value `age` armor
//!   (`-----BEGIN AGE ENCRYPTED FILE-----`) as the first-class format, plus
//!   whole-file rops (SOPS-compatible age) encryption for backward compatibility.
//! - Materializes decrypted secrets as `0600` files under `<data-dir>/secrets/` and
//!   passes only the file paths to the services, so secrets never appear in
//!   `ps`, systemd units, or logs.
//! - Generates command-line arguments for `alhalte-maitre-d` and `alhalte-cellar` as pure, deterministic functions.
//!   (`alhalte-banquette` is spawned per room by `alhalte-maitre-d`, which passes the room ID,
//!   administrator, socket path, and chunk size exponent itself.)
//! - Generates systemd unit configuration files (`alhalte-maitre-d.service`, `alhalte-cellar.service`, `alhalte-bistrot.target`) as pure functions.
//!
//! # Example
//!
//! ```
//! use alhalte_bistrot::config::BistrotConfig;
//! use alhalte_bistrot::args::generate_all_args;
//! use alhalte_bistrot::systemd::{generate_systemd_units, SystemdOptions};
//! use alhalte_bistrot::parse_bistrot_toml_with_options;
//! use alhalte_bistrot::rops::ParseOptions;
//!
//! let toml_str = r#"
//! [restaurant]
//! domain = "vtt.example.com"
//! data-dir = "/var/lib/alhalte"
//!
//! [maitre-d]
//! client-id = "my-client"
//! client-secret = "my-secret"
//! salt = "my-salt"
//!
//! [cellar]
//! anti-oracle = true
//! "#;
//!
//! // Plaintext secrets are fail-closed; the development escape hatch must be
//! // explicit. Production configurations store armored age values instead.
//! let options = ParseOptions { age_key: None, allow_plaintext_secrets: true };
//! let config = parse_bistrot_toml_with_options(toml_str, &options).unwrap();
//! let args = generate_all_args(&config);
//! assert!(args.cellar.contains(&"--anti-oracle".to_string()));
//! // The OAuth client secret is injected as a file path, not as a value.
//! assert!(args.maitre_d.contains(&"--client-secret-file".to_string()));
//! assert!(!args.maitre_d.contains(&"my-secret".to_string()));
//!
//! let units = generate_systemd_units(&config, &SystemdOptions::default());
//! assert!(units.maitre_d_service.contains("ExecStart="));
//! ```
//!
//! # Architecture Reference
//!
//! See `crates/alhalte-bistrot/ARCHITECTURE.md`.

#![deny(unsafe_code)]

pub mod args;
pub mod config;
pub mod error;
pub mod ffi;
pub mod lan;
pub mod rops;
pub mod secrets;
pub mod systemd;

// Re-exports for convenience
pub use args::{
    ModuleArgs, generate_all_args, generate_argenterie_args, generate_cellar_args,
    generate_maitre_d_args, generate_module_args,
};
pub use config::{
    ArgenterieConfig, BanquetteConfig, BistrotConfig, CellarConfig, DEFAULT_ARGENTERIE_PORT,
    DEFAULT_CHUNK_SIZE_EXPONENT, DEFAULT_DATA_DIR, DEFAULT_DOMAIN, DEFAULT_HOST,
    DEFAULT_SOCKET_DIR, ExtensionConfig, ExtensionManifestEntry, MaitreDConfig, RestaurantConfig,
};
pub use error::{BistrotError, Result};
pub use lan::{detect_lan_ipv4_addresses, detect_lan_urls, lan_url};
pub use rops::{
    ParseOptions, decrypt_rops_toml, decrypt_rops_toml_with_key, encrypt_age_armored_value,
    encrypt_bistrot_toml, is_rops_encrypted, load_from_path, load_from_path_with_key,
    load_from_path_with_options, parse_bistrot_toml, parse_bistrot_toml_with_key,
    parse_bistrot_toml_with_options,
};
pub use secrets::{
    AGE_ARMOR_BEGIN, AGE_ARMOR_END, ALLOW_PLAINTEXT_SECRETS_ENV, SecretFiles,
    allow_plaintext_secrets_from_env, decrypt_age_value, decrypt_age_values, encrypt_age_value,
    find_plaintext_secrets, is_age_armored_value, parse_age_identity, parse_age_recipient,
    secret_files, write_secret_files,
};
pub use systemd::{
    DEFAULT_SYSTEMD_BIN_DIR, SystemdOptions, SystemdUnits, generate_argenterie_service,
    generate_bistrot_target, generate_cellar_service, generate_maitre_d_service,
    generate_systemd_units,
};
