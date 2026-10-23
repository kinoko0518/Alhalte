//! `alhalte-bistrot` — pure `Restaurant.toml` to systemd unit conversion for
//! Alhalte.
//!
//! The crate owns only the systemd side:
//! - [`SystemdOptions`] / [`SystemdUnits`]
//! - [`generate_systemd_units`] and the per-service generators
//!
//! `Restaurant.toml` parsing, secret decryption, secret file materialization,
//! and argument generation live in `alhalte-restaurant` and are shared with
//! `alhalte-cafe`.
//!
//! # Example
//!
//! ```
//! use alhalte_bistrot::{SystemdOptions, generate_systemd_units};
//! use alhalte_restaurant::{ParseOptions, parse_restaurant_toml_with_options};
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
//! let options = ParseOptions { age_key: None, allow_plaintext_secrets: true };
//! let config = parse_restaurant_toml_with_options(toml_str, &options).unwrap();
//! let units = generate_systemd_units(&config, &SystemdOptions::default());
//! assert!(units.maitre_d_service.contains("ExecStart="));
//! assert!(units.target.contains("Wants=alhalte-cellar.service"));
//! ```
//!
//! # Architecture Reference
//!
//! See `crates/alhalte-bistrot/ARCHITECTURE.md`.

#![deny(unsafe_code)]

pub mod systemd;

pub use systemd::{
    DEFAULT_SYSTEMD_BIN_DIR, SystemdOptions, SystemdUnits, generate_alhalte_target,
    generate_cellar_service, generate_maitre_d_service, generate_systemd_units,
};
