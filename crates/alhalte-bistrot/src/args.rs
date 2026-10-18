//! Pure functions for generating command-line arguments for Alhalte modules.
//!
//! All functions in this module are deterministic and free of side effects.

use crate::config::BistrotConfig;
use crate::error::{BistrotError, Result};

/// Bundled arguments for all Alhalte modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleArgs {
    /// Command-line arguments for `alhalte-maitre-d`.
    pub maitre_d: Vec<String>,
    /// Command-line arguments for `alhalte-cellar`.
    pub cellar: Vec<String>,
    /// Command-line arguments for `alhalte-banquette`.
    pub banquette: Vec<String>,
}

/// Generates CLI arguments for `alhalte-maitre-d`.
///
/// Deterministic and pure function.
pub fn generate_maitre_d_args(config: &BistrotConfig) -> Vec<String> {
    let cellar_url = config
        .maitre_d
        .cellar_url
        .clone()
        .unwrap_or_else(|| config.cellar_url());

    let mut args = vec![
        "--host".to_string(),
        config.restaurant.host.clone(),
        "--port".to_string(),
        config.maitre_d.port.to_string(),
        "--db-path".to_string(),
        config.maitre_d_db_path().display().to_string(),
        "--client-id".to_string(),
        config.maitre_d.client_id.clone(),
        "--client-secret".to_string(),
        config.maitre_d.client_secret.clone(),
        "--salt".to_string(),
        config.maitre_d.salt.clone(),
        "--socket-dir".to_string(),
        config.maitre_d_socket_dir().display().to_string(),
        "--cellar-url".to_string(),
        cellar_url,
    ];

    if let Some(ref bin) = config.maitre_d.banquette_bin {
        args.push("--banquette-bin".to_string());
        args.push(bin.display().to_string());
    }

    args
}

/// Generates CLI arguments for `alhalte-cellar`.
///
/// Deterministic and pure function.
pub fn generate_cellar_args(config: &BistrotConfig) -> Vec<String> {
    let mut args = vec![
        "--host".to_string(),
        config.restaurant.host.clone(),
        "--port".to_string(),
        config.cellar.port.to_string(),
        "--storage-path".to_string(),
        config.cellar_storage_path().display().to_string(),
        "--db-path".to_string(),
        config.cellar_db_path().display().to_string(),
    ];

    if config.cellar.anti_oracle {
        args.push("--anti-oracle".to_string());
    }

    args
}

/// Generates CLI arguments for `alhalte-banquette`.
///
/// Deterministic and pure function.
pub fn generate_banquette_args(config: &BistrotConfig) -> Vec<String> {
    vec![
        "--chunk-size-exponent".to_string(),
        config.banquette.chunk_size_exponent.to_string(),
        "--socket-dir".to_string(),
        config.banquette_socket_dir().display().to_string(),
    ]
}

/// Generates CLI arguments for all Alhalte modules.
pub fn generate_all_args(config: &BistrotConfig) -> ModuleArgs {
    ModuleArgs {
        maitre_d: generate_maitre_d_args(config),
        cellar: generate_cellar_args(config),
        banquette: generate_banquette_args(config),
    }
}

/// Generates CLI arguments for a named module.
///
/// Supported module names: `"maitre-d"` (or `"maitre_d"`), `"cellar"`, `"banquette"`.
///
/// # Errors
///
/// Returns [`BistrotError::UnknownModule`] if the module name is not recognized.
pub fn generate_module_args(config: &BistrotConfig, module_name: &str) -> Result<Vec<String>> {
    match module_name.to_lowercase().replace('_', "-").as_str() {
        "maitre-d" => Ok(generate_maitre_d_args(config)),
        "cellar" => Ok(generate_cellar_args(config)),
        "banquette" => Ok(generate_banquette_args(config)),
        other => Err(BistrotError::UnknownModule(other.to_string())),
    }
}
