//! Configuration structures for `Restaurant.toml`.

use crate::error::{RestaurantError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default restaurant domain name.
pub const DEFAULT_DOMAIN: &str = "localhost";
/// Default restaurant host to bind on.
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// Default data directory for Alhalte.
#[cfg(not(windows))]
pub const DEFAULT_DATA_DIR: &str = "/var/lib/alhalte";
/// Default data directory for the portable Windows package.
#[cfg(windows)]
pub const DEFAULT_DATA_DIR: &str = "data";
/// Default socket directory for Alhalte UNIX sockets.
#[cfg(not(windows))]
pub const DEFAULT_SOCKET_DIR: &str = "/run/alhalte";
/// Default local transport directory for the portable Windows package.
#[cfg(windows)]
pub const DEFAULT_SOCKET_DIR: &str = "sockets";
/// Default port for maitre-d (and the bundled SPA).
pub const DEFAULT_MAITRE_D_PORT: u16 = 8080;
/// Default port for cellar.
pub const DEFAULT_CELLAR_PORT: u16 = 8081;
/// Default Vite development-server port.
///
/// The SPA is normally served by maitre-d itself. The Vite port is only used to
/// keep the local development origins in the CORS / `return_to` allow-list.
pub const DEFAULT_VITE_DEV_PORT: u16 = 5173;
/// Default log chunk size exponent N (1 << 8 = 256 logs per chunk).
pub const DEFAULT_CHUNK_SIZE_EXPONENT: u8 = 8;
/// Default banquette persistence / heartbeat interval in seconds.
pub const DEFAULT_BANQUETTE_HEARTBEAT_INTERVAL_SECS: u64 = 240;

fn default_domain() -> String {
    DEFAULT_DOMAIN.to_string()
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

fn default_data_dir() -> PathBuf {
    PathBuf::from(DEFAULT_DATA_DIR)
}

fn default_socket_dir() -> PathBuf {
    PathBuf::from(DEFAULT_SOCKET_DIR)
}

fn default_maitre_d_port() -> u16 {
    DEFAULT_MAITRE_D_PORT
}

fn default_cellar_port() -> u16 {
    DEFAULT_CELLAR_PORT
}

fn default_anti_oracle() -> bool {
    false
}

fn default_chunk_size_exponent() -> u8 {
    DEFAULT_CHUNK_SIZE_EXPONENT
}

fn default_banquette_heartbeat_interval_secs() -> u64 {
    DEFAULT_BANQUETTE_HEARTBEAT_INTERVAL_SECS
}

fn default_extension_enabled() -> bool {
    true
}

/// Returns true when `domain` addresses the local machine.
///
/// Loopback development serves the UI over HTTP on a dev port, so it must not
/// be upgraded to `https://{domain}` when deriving the public URL.
fn is_loopback_domain(domain: &str) -> bool {
    let trimmed = domain.trim();
    if trimmed.eq_ignore_ascii_case("localhost") || trimmed == "::1" || trimmed == "[::1]" {
        return true;
    }
    let host = trimmed.split(':').next().unwrap_or(trimmed);
    host.eq_ignore_ascii_case("localhost") || host.starts_with("127.")
}

/// Returns true when `host` is a loopback address or name.
fn is_loopback_host(host: &str) -> bool {
    is_loopback_domain(host)
}

/// Returns true when `host` binds every interface.
fn is_wildcard_host(host: &str) -> bool {
    matches!(host.trim(), "0.0.0.0" | "::" | "[::]")
}

fn absolutize(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    // `std::path::absolute` resolves a relative base (for example `.` when the
    // configuration file is in the process cwd) without requiring the path to
    // exist. Fall back to the joined path if the cwd is unavailable.
    std::path::absolute(&joined).unwrap_or(joined)
}

/// Root configuration for the entire Alhalte restaurant instance
/// (`Restaurant.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RestaurantToml {
    /// Restaurant general configuration.
    #[serde(default)]
    pub restaurant: RestaurantConfig,

    /// Configuration for `alhalte-maitre-d` module.
    #[serde(alias = "maitre_d")]
    pub maitre_d: MaitreDConfig,

    /// Configuration for `alhalte-cellar` module.
    pub cellar: CellarConfig,

    /// Configuration for `alhalte-banquette` module.
    #[serde(default)]
    pub banquette: BanquetteConfig,

    /// Default extensions list configured for the restaurant.
    #[serde(default)]
    pub extensions: Vec<ExtensionConfig>,
}

impl RestaurantToml {
    /// Validates the configuration values.
    ///
    /// # Errors
    ///
    /// Returns [`RestaurantError::ConfigValidation`] if any required field is
    /// missing or invalid.
    pub fn validate(&self) -> Result<()> {
        if self.restaurant.domain.trim().is_empty() {
            return Err(RestaurantError::ConfigValidation(
                "restaurant domain must not be empty".to_string(),
            ));
        }
        if self.restaurant.host.trim().is_empty() {
            return Err(RestaurantError::ConfigValidation(
                "restaurant host must not be empty".to_string(),
            ));
        }
        if self.restaurant.data_dir.as_os_str().is_empty() {
            return Err(RestaurantError::ConfigValidation(
                "restaurant data-dir must not be empty".to_string(),
            ));
        }
        if self.restaurant.socket_dir.as_os_str().is_empty() {
            return Err(RestaurantError::ConfigValidation(
                "restaurant socket-dir must not be empty".to_string(),
            ));
        }
        if self.maitre_d.port == 0 {
            return Err(RestaurantError::ConfigValidation(
                "maitre-d port must not be 0".to_string(),
            ));
        }
        if self.maitre_d.salt.trim().is_empty() {
            return Err(RestaurantError::ConfigValidation(
                "maitre-d instance salt must not be empty".to_string(),
            ));
        }
        // Google OAuth needs both credentials. Local password login does not.
        if !self.maitre_d.local_login {
            if self.maitre_d.client_id.trim().is_empty() {
                return Err(RestaurantError::ConfigValidation(
                    "maitre-d Google OAuth client-id must not be empty".to_string(),
                ));
            }
            if self.maitre_d.client_secret.trim().is_empty() {
                return Err(RestaurantError::ConfigValidation(
                    "maitre-d Google OAuth client-secret must not be empty".to_string(),
                ));
            }
        }
        if let Some(url) = &self.maitre_d.public_url
            && url.trim().is_empty()
        {
            return Err(RestaurantError::ConfigValidation(
                "maitre-d public-url must not be empty when set".to_string(),
            ));
        }
        if self.cellar.port == 0 {
            return Err(RestaurantError::ConfigValidation(
                "cellar port must not be 0".to_string(),
            ));
        }
        if self.banquette.chunk_size_exponent == 0 || self.banquette.chunk_size_exponent > 30 {
            return Err(RestaurantError::ConfigValidation(
                "banquette chunk-size-exponent must be between 1 and 30".to_string(),
            ));
        }
        if self.banquette.heartbeat_interval_secs == 0 {
            return Err(RestaurantError::ConfigValidation(
                "banquette heartbeat-interval-secs must be greater than 0".to_string(),
            ));
        }
        if let Some(token) = &self.cellar.service_token
            && token.trim().is_empty()
        {
            return Err(RestaurantError::ConfigValidation(
                "cellar service-token must not be empty when set".to_string(),
            ));
        }
        if self.extensions.iter().any(|ext| ext.id.trim().is_empty()) {
            return Err(RestaurantError::ConfigValidation(
                "extension id must not be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Returns the host to dial for locally generated URLs.
    ///
    /// Wildcard bind addresses (`0.0.0.0` and `::`) are not connectable, so
    /// they are replaced with the loopback address.
    fn connect_host(&self) -> &str {
        match self.restaurant.host.as_str() {
            "0.0.0.0" | "::" => "127.0.0.1",
            host => host,
        }
    }

    /// Resolves `data-dir` and `socket-dir` against the configuration file's
    /// directory so the binaries do not depend on the process cwd.
    pub fn resolve_relative_paths(&mut self, base: &Path) {
        self.restaurant.data_dir = absolutize(base, &self.restaurant.data_dir);
        self.restaurant.socket_dir = absolutize(base, &self.restaurant.socket_dir);
    }

    /// Resolves the URL for the bundled UI in a browser.
    ///
    /// The UI is served by `maitre-d` on the same origin as `/api` and `/auth`,
    /// so this is the `maitre-d` URL. It stays loopback even when the instance
    /// is reachable from the LAN; `alhalte-cafe` performs the best-effort LAN
    /// URL detection.
    pub fn maitre_d_url(&self) -> String {
        format!("http://{}:{}/", self.connect_host(), self.maitre_d.port)
    }

    /// Public base URL of `alhalte-maitre-d` as seen by browsers.
    ///
    /// Resolution order:
    /// 1. `maitre-d.public-url` when configured.
    /// 2. `https://{restaurant.domain}` when the domain is a real host (the
    ///    production reverse proxy serves the UI and maitre-d together).
    /// 3. The loopback `maitre-d` URL for local development.
    pub fn maitre_d_public_url(&self) -> String {
        if let Some(url) = &self.maitre_d.public_url {
            let trimmed = url.trim().trim_end_matches('/');
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        let domain = self.restaurant.domain.trim().trim_end_matches('/');
        if !domain.is_empty() && !is_loopback_domain(domain) {
            return format!("https://{domain}");
        }

        self.maitre_d_url().trim_end_matches('/').to_string()
    }

    /// Browser origins `alhalte-maitre-d` should allow through CORS and OAuth
    /// `return_to`.
    ///
    /// The UI normally shares an origin with maitre-d, but the explicit
    /// allow-list is still needed for the OAuth return URL and for the optional
    /// Vite development server (`http://127.0.0.1:5173`). CORS never falls back
    /// to a wildcard.
    pub fn cors_origins(&self) -> Vec<String> {
        let mut origins = Vec::new();
        let mut push = |origin: String| {
            if !origin.is_empty() && !origins.contains(&origin) {
                origins.push(origin);
            }
        };

        push(self.maitre_d_public_url().trim_end_matches('/').to_string());

        let domain = self.restaurant.domain.trim().trim_end_matches('/');
        if !domain.is_empty() {
            push(format!("http://{domain}"));
            push(format!("https://{domain}"));
            push(format!("http://{domain}:{}", self.maitre_d.port));
            push(format!("https://{domain}:{}", self.maitre_d.port));
        }

        // The optional Vite dev server runs on a different origin. Keep the
        // local origins allowed for loopback instances so `pnpm dev` still
        // works while the resident UI is served by maitre-d.
        if is_loopback_host(&self.restaurant.host) || is_loopback_domain(&self.restaurant.domain) {
            push(format!("http://127.0.0.1:{DEFAULT_VITE_DEV_PORT}"));
            push(format!("http://localhost:{DEFAULT_VITE_DEV_PORT}"));
        }

        origins
    }

    /// Returns true when the instance binds beyond the loopback interface.
    pub fn is_lan_exposed(&self) -> bool {
        let host = self.restaurant.host.trim();
        is_wildcard_host(host) || (!host.is_empty() && !is_loopback_host(host))
    }

    /// Ports on which `maitre-d` should accept private-network browser origins.
    ///
    /// Empty unless the instance is exposed beyond loopback. When exposed, the
    /// UI and API share the `maitre-d` port, so that is the only port needed.
    pub fn lan_origin_ports(&self) -> Vec<u16> {
        if self.is_lan_exposed() {
            vec![self.maitre_d.port]
        } else {
            Vec::new()
        }
    }

    /// Resolves the HTTP URL for `cellar`.
    ///
    /// cellar is an internal service and always binds loopback.
    pub fn cellar_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.cellar.port)
    }

    /// Resolves the `maitre-d` URL that `cellar` uses to verify sessions.
    pub fn cellar_maitre_d_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.maitre_d.port)
    }

    /// Resolves the SQLite database path for `maitre-d`.
    pub fn maitre_d_db_path(&self) -> PathBuf {
        self.restaurant.data_dir.join("maitre-d.sqlite3")
    }

    /// Resolves the UNIX socket directory for `maitre-d`.
    pub fn maitre_d_socket_dir(&self) -> PathBuf {
        self.restaurant.socket_dir.clone()
    }

    /// Resolves the storage path for `cellar` tin data.
    pub fn cellar_storage_path(&self) -> PathBuf {
        self.restaurant.data_dir.join("cellar").join("tin")
    }

    /// Resolves the database path for `cellar`.
    pub fn cellar_db_path(&self) -> PathBuf {
        self.restaurant
            .data_dir
            .join("cellar")
            .join("cellar.sqlite3")
    }

    /// Resolves the UNIX socket directory for `banquette`.
    pub fn banquette_socket_dir(&self) -> PathBuf {
        self.restaurant.socket_dir.clone()
    }

    /// Directory where decrypted secrets are materialized (`<data-dir>/secrets`).
    ///
    /// The restaurant layer writes the decrypted secrets there with mode `0600`
    /// and passes the file paths to the services, so secrets never appear in
    /// `ps`, systemd units, or logs (Issue #194).
    pub fn secrets_dir(&self) -> PathBuf {
        self.restaurant.data_dir.join("secrets")
    }

    /// Stages configured extensions into the specified target directory and
    /// generates `manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns [`RestaurantError::ExtensionWasmMissing`] when a configured
    /// `.wasm` does not exist, and [`RestaurantError`] for filesystem errors.
    pub fn stage_extensions(
        &self,
        output_dir: impl AsRef<std::path::Path>,
    ) -> Result<Vec<ExtensionManifestEntry>> {
        self.stage_extensions_with_base(output_dir, None)
    }

    /// Stages configured extensions, resolving relative paths against
    /// `base_dir` when the configured path does not exist.
    ///
    /// Every configured `.wasm` is resolved before anything is copied, so a
    /// configuration error cannot leave a partially staged directory or a
    /// manifest entry pointing at a missing wasm.
    ///
    /// # Errors
    ///
    /// Returns [`RestaurantError::ExtensionWasmMissing`] when a configured wasm
    /// does not exist. In that case nothing is copied and `manifest.json` is not
    /// written.
    pub fn stage_extensions_with_base(
        &self,
        output_dir: impl AsRef<std::path::Path>,
        base_dir: Option<&std::path::Path>,
    ) -> Result<Vec<ExtensionManifestEntry>> {
        let out_path = output_dir.as_ref();

        let mut sources = Vec::with_capacity(self.extensions.len());
        for ext in &self.extensions {
            let configured = ext.path.clone();
            let checked = if configured.exists() {
                configured.clone()
            } else if let Some(base) = base_dir {
                let candidate = base.join(&configured);
                if candidate.exists() {
                    candidate
                } else {
                    return Err(RestaurantError::ExtensionWasmMissing {
                        id: ext.id.clone(),
                        path: candidate,
                    });
                }
            } else {
                return Err(RestaurantError::ExtensionWasmMissing {
                    id: ext.id.clone(),
                    path: configured,
                });
            };
            sources.push(checked);
        }

        if !out_path.exists() {
            std::fs::create_dir_all(out_path)?;
        }

        let mut manifest = Vec::with_capacity(self.extensions.len());
        for (priority, (ext, src)) in self.extensions.iter().zip(sources).enumerate() {
            let dest_wasm_name = format!("{}.wasm", ext.id);
            let dest_wasm_path = out_path.join(&dest_wasm_name);
            std::fs::copy(&src, &dest_wasm_path)?;

            manifest.push(ExtensionManifestEntry {
                id: ext.id.clone(),
                url: format!("/extensions/{dest_wasm_name}"),
                enabled: ext.enabled,
                priority,
            });
        }

        let manifest_path = out_path.join("manifest.json");
        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, json)?;

        Ok(manifest)
    }
}

/// Restaurant global settings (instance host, domain, base directories).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RestaurantConfig {
    /// Domain name of this restaurant instance (e.g. `"vtt.example.com"`).
    #[serde(default = "default_domain")]
    pub domain: String,

    /// IP or hostname to bind listeners on (e.g. `"127.0.0.1"` or `"0.0.0.0"`).
    #[serde(default = "default_host")]
    pub host: String,

    /// Base data directory for persistent files.
    #[serde(default = "default_data_dir", alias = "data_dir")]
    pub data_dir: PathBuf,

    /// Base directory for UNIX sockets.
    #[serde(default = "default_socket_dir", alias = "socket_dir")]
    pub socket_dir: PathBuf,
}

impl Default for RestaurantConfig {
    fn default() -> Self {
        Self {
            domain: default_domain(),
            host: default_host(),
            data_dir: default_data_dir(),
            socket_dir: default_socket_dir(),
        }
    }
}

/// Configuration for `alhalte-maitre-d` (OAuth authentication and WebSocket
/// gateway).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MaitreDConfig {
    /// Listening HTTP/WebSocket port (also the bundled SPA port).
    #[serde(default = "default_maitre_d_port")]
    pub port: u16,

    /// Google OAuth Client ID.
    ///
    /// Required unless `local-login` is enabled.
    #[serde(default, alias = "client_id")]
    pub client_id: String,

    /// Google OAuth Client Secret (decrypted from age armor in production).
    ///
    /// Required unless `local-login` is enabled.
    #[serde(default, alias = "client_secret")]
    pub client_secret: String,

    /// Instance salt used for encrypting Google account sub and local account
    /// names.
    #[serde(default)]
    pub salt: String,

    /// Optional public base URL of `maitre-d` as seen by browsers.
    ///
    /// Google OAuth redirects back to `{public-url}/auth/google/callback`. When
    /// omitted it is derived from `restaurant.domain` (production) or the
    /// loopback `maitre-d` URL (development).
    #[serde(default, alias = "public_url")]
    pub public_url: Option<String>,

    /// Enables the local password login (`maitre-d --local-login`).
    ///
    /// Local accounts, sessions, and the room registry live in
    /// `maitre-d-local.sqlite3` next to the normal database. Room state is still
    /// persisted to cellar. The login is HTTP-only, so use it only on a trusted
    /// LAN.
    #[serde(default, alias = "local_login")]
    pub local_login: bool,
}

/// Configuration for `alhalte-cellar` (file storage and tin deduplication).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CellarConfig {
    /// Listening port for cellar HTTP API.
    #[serde(default = "default_cellar_port")]
    pub port: u16,

    /// Anti-oracle mode flag (when true, hides duplicate detection from clients
    /// to prevent oracle attacks).
    #[serde(default = "default_anti_oracle", alias = "anti_oracle")]
    pub anti_oracle: bool,

    /// Internal service token shared by `maitre-d` / `banquette` / `cellar`.
    ///
    /// age-managed secret. `maitre-d` forwards it to every spawned banquette,
    /// which sends it as `X-Alhalte-Service-Token` for room state persistence;
    /// `cellar` accepts it for internal store / unregister calls while still
    /// verifying user sessions for the public proxy.
    #[serde(default, alias = "service_token")]
    pub service_token: Option<String>,
}

/// Configuration for `alhalte-banquette` (in-memory room session processes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BanquetteConfig {
    /// Log chunk size exponent N (where 1 << N is the number of logs per chunk,
    /// default 8 = 256).
    #[serde(default = "default_chunk_size_exponent", alias = "chunk_size_exponent")]
    pub chunk_size_exponent: u8,

    /// Heartbeat / persistence interval forwarded to every banquette.
    ///
    /// Defaults to 240 seconds. The portable package uses 60 seconds so a
    /// forced shutdown loses at most about one minute of room state.
    #[serde(default = "default_banquette_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
}

impl Default for BanquetteConfig {
    fn default() -> Self {
        Self {
            chunk_size_exponent: default_chunk_size_exponent(),
            heartbeat_interval_secs: default_banquette_heartbeat_interval_secs(),
        }
    }
}

/// Default extension entry configured in `Restaurant.toml`.
///
/// The display name / icon / description are **not** configured here: they come
/// from the extension wasm's `self_introduction` export (Issue #274), so the
/// manifest and the UI have a single source of truth. `id` must stay in sync
/// with the id the wasm itself reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ExtensionConfig {
    /// Unique identifier for the extension (e.g. `"alhalte-dsl"`).
    pub id: String,

    /// Path to the compiled `.wasm` binary (relative to the configuration file
    /// or absolute).
    pub path: PathBuf,

    /// Whether the extension is enabled by default in rooms.
    #[serde(default = "default_extension_enabled")]
    pub enabled: bool,
}

/// Extension manifest entry written into `manifest.json`.
///
/// Display metadata is intentionally absent: `alhalte-argenterie` reads it from
/// the staged wasm (`self_introduction`, ABI 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExtensionManifestEntry {
    /// Unique identifier for the extension (e.g. `"alhalte-dsl"`).
    pub id: String,

    /// URL to download the extension `.wasm` file
    /// (e.g. `"/extensions/alhalte-dsl.wasm"`).
    pub url: String,

    /// Whether the extension is enabled by default in rooms.
    pub enabled: bool,

    /// Execution priority order index.
    pub priority: usize,
}
