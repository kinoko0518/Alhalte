//! `alhalte-bistrot` — Restaurant configuration manager, argument generator, and systemd generator for Alhalte.
//!
//! # Overview
//!
//! `alhalte-bistrot` serves as the orchestration and configuration manager for an Alhalte restaurant instance:
//! - Parses and validates `Bistrot.toml` configuration files.
//! - Decrypts encrypted secrets (Google OAuth secrets, instance salt, etc.) using `rops` (SOPS-compatible age encryption).
//! - Generates command-line arguments for `alhalte-maitre-d`, `alhalte-cellar`, and `alhalte-banquette` as pure, deterministic functions.
//! - Generates systemd unit configuration files (`alhalte-maitre-d.service`, `alhalte-cellar.service`, `alhalte-bistrot.target`) as pure functions.
//!
//! # Example
//!
//! ```
//! use alhalte_bistrot::config::BistrotConfig;
//! use alhalte_bistrot::args::generate_all_args;
//! use alhalte_bistrot::systemd::{generate_systemd_units, SystemdOptions};
//! use alhalte_bistrot::parse_bistrot_toml;
//!
//! let toml_str = r#"
//! [restaurant]
//! domain = "vtt.example.com"
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
//! let config = parse_bistrot_toml(toml_str).unwrap();
//! let args = generate_all_args(&config);
//! assert!(args.cellar.contains(&"--anti-oracle".to_string()));
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
pub mod rops;
pub mod systemd;

// Re-exports for convenience
pub use args::{
    ModuleArgs, generate_all_args, generate_banquette_args, generate_cellar_args,
    generate_maitre_d_args, generate_module_args,
};
pub use config::{
    ArgenterieConfig, BanquetteConfig, BistrotConfig, CellarConfig, DEFAULT_ARGENTERIE_PORT,
    DEFAULT_CHUNK_SIZE_EXPONENT, DEFAULT_DATA_DIR, DEFAULT_DOMAIN, DEFAULT_HOST,
    DEFAULT_SOCKET_DIR, ExtensionConfig, ExtensionManifestEntry, MaitreDConfig, RestaurantConfig,
};
pub use error::{BistrotError, Result};
pub use rops::{
    decrypt_rops_toml, decrypt_rops_toml_with_key, encrypt_bistrot_toml, is_rops_encrypted,
    load_from_path, load_from_path_with_key, parse_bistrot_toml, parse_bistrot_toml_with_key,
};
pub use systemd::{
    DEFAULT_SYSTEMD_BIN_DIR, SystemdOptions, SystemdUnits, generate_argenterie_service,
    generate_bistrot_target, generate_cellar_service, generate_maitre_d_service,
    generate_systemd_units,
};
