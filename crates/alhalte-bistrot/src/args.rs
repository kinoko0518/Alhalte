//! Pure functions for generating command-line arguments for Alhalte modules.
//!
//! All functions in this module are deterministic and free of side effects.
//!
//! Secrets are passed as **file paths**, never as values (Issue #194):
//! `maitre-d.client-secret` becomes `--client-secret-file <data-dir>/secrets/
//! maitre-d-client-secret` and `cellar.service-token` becomes
//! `--service-token-file <data-dir>/secrets/cellar-service-token`. Bistrot writes
//! those files with mode `0600` via [`crate::secrets::write_secret_files`] before
//! the services are started, so neither `ps` nor the generated systemd units see
//! the secret.

use crate::config::BistrotConfig;
use crate::error::{BistrotError, Result};
use crate::secrets::secret_files;

/// Bundled arguments for the long-lived Alhalte services.
///
/// `alhalte-banquette` is not included: it is spawned per room by
/// `alhalte-maitre-d`, which supplies `--room-id`, `--admin`, `--socket`, and
/// the chunk size exponent itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleArgs {
    /// Command-line arguments for `alhalte-maitre-d`.
    pub maitre_d: Vec<String>,
    /// Command-line arguments for `alhalte-cellar`.
    pub cellar: Vec<String>,
}

/// Generates CLI arguments for `alhalte-maitre-d`.
///
/// `[banquette] chunk-size-exponent` is passed as `--chunk-size-exponent` so
/// maitre-d can forward it to every banquette process it spawns.
///
/// The OAuth client secret is passed as `--client-secret-file <path>` and the
/// internal service token as `--service-token-file <path>`; the values never
/// appear in the argument list (Issue #194).
///
/// Deterministic and pure function.
pub fn generate_maitre_d_args(config: &BistrotConfig) -> Vec<String> {
    let cellar_url = config
        .maitre_d
        .cellar_url
        .clone()
        .unwrap_or_else(|| config.cellar_url());
    let secrets = secret_files(config);

    let mut args = vec![
        "--host".to_string(),
        config.restaurant.host.clone(),
        "--port".to_string(),
        config.maitre_d.port.to_string(),
        "--db-path".to_string(),
        config.maitre_d_db_path().display().to_string(),
        "--client-id".to_string(),
        config.maitre_d.client_id.clone(),
        "--salt".to_string(),
        config.maitre_d.salt.clone(),
        // Fixed Google OAuth redirect base; the argenterie origin is not part
        // of the redirect URI.
        "--public-url".to_string(),
        config.maitre_d_public_url(),
        "--socket-dir".to_string(),
        config.maitre_d_socket_dir().display().to_string(),
        "--cellar-url".to_string(),
        cellar_url,
        // Passed through to every banquette process maitre-d spawns.
        "--chunk-size-exponent".to_string(),
        config.banquette.chunk_size_exponent.to_string(),
    ];

    if let Some(path) = secrets.maitre_d_client_secret {
        args.push("--client-secret-file".to_string());
        args.push(path.display().to_string());
    }

    if let Some(ref bin) = config.maitre_d.banquette_bin {
        args.push("--banquette-bin".to_string());
        args.push(bin.display().to_string());
    }

    if let Some(path) = secrets.service_token {
        args.push("--service-token-file".to_string());
        args.push(path.display().to_string());
    }

    // CORS is an explicit allow-list in maitre-d; derive it from the
    // argenterie URL / restaurant domain instead of defaulting to permissive.
    for origin in config.cors_origins() {
        args.push("--cors-origin".to_string());
        args.push(origin);
    }

    // Issue #150: debug login is opt-in. maitre-d then picks the isolated
    // `<data-dir>/maitre-d-debug.sqlite3` database itself, so no extra path is
    // passed here.
    if config.maitre_d.debug_login {
        args.push("--debug-login".to_string());
    }

    args
}

/// Generates CLI arguments for `alhalte-cellar`.
///
/// The session verifier is configured through `--maitre-d-url` (the explicit
/// `cellar.maitre-d-url`, or the URL derived from `restaurant.host` and
/// `maitre-d.port`) or through `--maitre-d-socket` when
/// `cellar.maitre-d-socket` is set. The socket takes precedence, matching
/// `alhalte-cellar`'s own CLI behavior. `--allow-all-sessions` is emitted only
/// when explicitly enabled in the configuration.
///
/// The internal service token is passed as `--service-token-file <path>`, never
/// as a value (Issue #194).
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

    if let Some(ref socket) = config.cellar.maitre_d_socket {
        args.push("--maitre-d-socket".to_string());
        args.push(socket.display().to_string());
    } else {
        args.push("--maitre-d-url".to_string());
        args.push(config.cellar_maitre_d_url());
    }

    if config.cellar.allow_all_sessions {
        args.push("--allow-all-sessions".to_string());
    }

    if config.cellar.anti_oracle {
        args.push("--anti-oracle".to_string());
    }

    if let Some(path) = secret_files(config).service_token {
        args.push("--service-token-file".to_string());
        args.push(path.display().to_string());
    }

    args
}

/// Generates CLI arguments for all long-lived Alhalte services.
pub fn generate_all_args(config: &BistrotConfig) -> ModuleArgs {
    ModuleArgs {
        maitre_d: generate_maitre_d_args(config),
        cellar: generate_cellar_args(config),
    }
}

/// Generates CLI arguments for a named module.
///
/// Supported module names: `"maitre-d"` (or `"maitre_d"`) and `"cellar"`.
/// `alhalte-banquette` is spawned per room by `alhalte-maitre-d`, so it has no
/// static argument list here.
///
/// # Errors
///
/// Returns [`BistrotError::UnknownModule`] if the module name is not recognized.
pub fn generate_module_args(config: &BistrotConfig, module_name: &str) -> Result<Vec<String>> {
    match module_name.to_lowercase().replace('_', "-").as_str() {
        "maitre-d" => Ok(generate_maitre_d_args(config)),
        "cellar" => Ok(generate_cellar_args(config)),
        other => Err(BistrotError::UnknownModule(other.to_string())),
    }
}
