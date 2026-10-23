//! Pure functions for generating command-line arguments for Alhalte modules.
//!
//! All functions in this module are deterministic and free of side effects.
//!
//! Secrets are passed as **file paths**, never as values (Issue #194):
//! `maitre-d.client-secret` becomes `--client-secret-file <data-dir>/secrets/
//! maitre-d-client-secret` and `cellar.service-token` becomes
//! `--service-token-file <data-dir>/secrets/cellar-service-token`. The
//! restaurant layer writes those files with mode `0600` via
//! [`crate::secrets::write_secret_files`] before the services are started, so
//! neither `ps` nor the generated systemd units see the secret.

use crate::config::RestaurantToml;
use crate::secrets::secret_files;
use std::path::PathBuf;

/// Execution-time placement options that are not part of `Restaurant.toml`.
///
/// The bundled SPA and the `alhalte-banquette` binary live next to the `cafe`
/// executable or in a deployment directory, so they are passed as argument
/// options rather than configuration fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaitreDArgsOptions {
    /// Directory containing the built SPA (`maitre-d --web-root`).
    pub web_root: Option<PathBuf>,
    /// Optional explicit `alhalte-banquette` binary path.
    pub banquette_bin: Option<PathBuf>,
}

/// Bundled arguments for the long-lived Alhalte services.
///
/// `alhalte-banquette` is not included: it is spawned per room by
/// `alhalte-maitre-d`, which supplies `--room-id`, `--admin`, the local transport
/// endpoint (UNIX socket or Windows named pipe), and the chunk size exponent
/// itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleArgs {
    /// Command-line arguments for `alhalte-maitre-d`.
    pub maitre_d: Vec<String>,
    /// Command-line arguments for `alhalte-cellar`.
    pub cellar: Vec<String>,
}

/// Generates CLI arguments for `alhalte-maitre-d`.
///
/// `[banquette] chunk-size-exponent` and `heartbeat-interval-secs` are passed so
/// maitre-d can forward them to every banquette process it spawns.
///
/// `[maitre-d] local-login` emits `--local-login` (separate local SQLite file,
/// persistent rooms). `web-root` and `banquette-bin` come from
/// [`MaitreDArgsOptions`] because they describe where the running instance is
/// deployed, not the instance configuration.
///
/// The OAuth client secret is passed as `--client-secret-file <path>` and the
/// internal service token as `--service-token-file <path>`; the values never
/// appear in the argument list (Issue #194).
///
/// Deterministic and pure function.
pub fn generate_maitre_d_args(
    config: &RestaurantToml,
    options: &MaitreDArgsOptions,
) -> Vec<String> {
    let cellar_url = config.cellar_url();
    let secrets = secret_files(config);

    let mut args = vec![
        "--host".to_string(),
        config.restaurant.host.clone(),
        "--port".to_string(),
        config.maitre_d.port.to_string(),
        "--db-path".to_string(),
        config.maitre_d_db_path().display().to_string(),
        "--salt".to_string(),
        config.maitre_d.salt.clone(),
        // Fixed public base URL; the UI origin is not part of the redirect URI.
        "--public-url".to_string(),
        config.maitre_d_public_url(),
        "--socket-dir".to_string(),
        config.maitre_d_socket_dir().display().to_string(),
        "--cellar-url".to_string(),
        cellar_url,
        // Passed through to every banquette process maitre-d spawns.
        "--chunk-size-exponent".to_string(),
        config.banquette.chunk_size_exponent.to_string(),
        "--banquette-heartbeat-interval-secs".to_string(),
        config.banquette.heartbeat_interval_secs.to_string(),
    ];

    if !config.maitre_d.client_id.trim().is_empty() {
        args.push("--client-id".to_string());
        args.push(config.maitre_d.client_id.clone());
    }

    if let Some(path) = secrets.maitre_d_client_secret {
        args.push("--client-secret-file".to_string());
        args.push(path.display().to_string());
    }

    if let Some(ref bin) = options.banquette_bin {
        args.push("--banquette-bin".to_string());
        args.push(bin.display().to_string());
    }

    if let Some(path) = secrets.service_token {
        args.push("--service-token-file".to_string());
        args.push(path.display().to_string());
    }

    // CORS is an explicit allow-list in maitre-d; derive it from the public URL
    // and restaurant domain instead of defaulting to permissive.
    for origin in config.cors_origins() {
        args.push("--cors-origin".to_string());
        args.push(origin);
    }

    // Issue #229: when the instance is exposed beyond loopback, accept
    // `http://<private IPv4>:<maitre-d.port>` browser origins for the same
    // Ethernet/LAN. This is not a wildcard: maitre-d only accepts private
    // network IP literals on the listed port.
    for port in config.lan_origin_ports() {
        args.push("--allow-lan-origin-port".to_string());
        args.push(port.to_string());
    }

    // Local password login: maitre-d uses a separate SQLite file and the SPA
    // `/login` route. Room state is still persisted to cellar.
    if config.maitre_d.local_login {
        args.push("--local-login".to_string());
    }
    if let Some(path) = &options.web_root {
        args.push("--web-root".to_string());
        args.push(path.display().to_string());
    }

    args
}

/// Generates CLI arguments for `alhalte-cellar`.
///
/// cellar is an internal service: it always binds `127.0.0.1`, and its session
/// verifier always points at `http://127.0.0.1:{maitre-d.port}`.
///
/// The internal service token is passed as `--service-token-file <path>`, never
/// as a value (Issue #194).
///
/// Deterministic and pure function.
pub fn generate_cellar_args(config: &RestaurantToml) -> Vec<String> {
    let mut args = vec![
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        config.cellar.port.to_string(),
        "--storage-path".to_string(),
        config.cellar_storage_path().display().to_string(),
        "--db-path".to_string(),
        config.cellar_db_path().display().to_string(),
        "--maitre-d-url".to_string(),
        config.cellar_maitre_d_url(),
    ];

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
pub fn generate_all_args(config: &RestaurantToml, options: &MaitreDArgsOptions) -> ModuleArgs {
    ModuleArgs {
        maitre_d: generate_maitre_d_args(config, options),
        cellar: generate_cellar_args(config),
    }
}
