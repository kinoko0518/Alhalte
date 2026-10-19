//! Configuration structures for `Bistrot.toml`.

use crate::error::{BistrotError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default restaurant domain name.
pub const DEFAULT_DOMAIN: &str = "localhost";
/// Default restaurant host to bind on.
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// Default data directory for Alhalte.
pub const DEFAULT_DATA_DIR: &str = "/var/lib/alhalte";
/// Default socket directory for Alhalte UNIX sockets.
pub const DEFAULT_SOCKET_DIR: &str = "/run/alhalte";
/// Default port for maitre-d.
pub const DEFAULT_MAITRE_D_PORT: u16 = 8080;
/// Default port for cellar.
pub const DEFAULT_CELLAR_PORT: u16 = 8081;
/// Default port for argenterie development server.
pub const DEFAULT_ARGENTERIE_PORT: u16 = 5173;
/// Default log chunk size exponent N (1 << 8 = 256 logs per chunk).
pub const DEFAULT_CHUNK_SIZE_EXPONENT: u8 = 8;

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

fn default_argenterie_port() -> u16 {
    DEFAULT_ARGENTERIE_PORT
}

fn default_anti_oracle() -> bool {
    false
}

fn default_chunk_size_exponent() -> u8 {
    DEFAULT_CHUNK_SIZE_EXPONENT
}

fn default_extension_enabled() -> bool {
    true
}

/// Returns true when `domain` addresses the local machine.
///
/// Loopback development serves argenterie over HTTP on a dev port, so it must
/// not be upgraded to `https://{domain}` when deriving the public URL.
fn is_loopback_domain(domain: &str) -> bool {
    let trimmed = domain.trim();
    if trimmed.eq_ignore_ascii_case("localhost") || trimmed == "::1" || trimmed == "[::1]" {
        return true;
    }
    let host = trimmed.split(':').next().unwrap_or(trimmed);
    host.eq_ignore_ascii_case("localhost") || host.starts_with("127.")
}

/// Root configuration for the entire Alhalte restaurant instance (`Bistrot.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BistrotConfig {
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

    /// Configuration for `alhalte-argenterie` web frontend module.
    #[serde(default)]
    pub argenterie: ArgenterieConfig,

    /// Default extensions list configured for the restaurant.
    #[serde(default)]
    pub extensions: Vec<ExtensionConfig>,
}

impl BistrotConfig {
    /// Validates the configuration values.
    ///
    /// # Errors
    ///
    /// Returns [`BistrotError::ConfigValidation`] if any required secret or field is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.maitre_d.client_id.trim().is_empty() {
            return Err(BistrotError::ConfigValidation(
                "maitre-d Google OAuth client-id must not be empty".to_string(),
            ));
        }
        if self.maitre_d.client_secret.trim().is_empty() {
            return Err(BistrotError::ConfigValidation(
                "maitre-d Google OAuth client-secret must not be empty".to_string(),
            ));
        }
        if self.maitre_d.salt.trim().is_empty() {
            return Err(BistrotError::ConfigValidation(
                "maitre-d instance salt must not be empty".to_string(),
            ));
        }
        if let Some(url) = &self.maitre_d.public_url
            && url.trim().is_empty()
        {
            return Err(BistrotError::ConfigValidation(
                "maitre-d public-url must not be empty when set".to_string(),
            ));
        }
        if self.banquette.chunk_size_exponent == 0 || self.banquette.chunk_size_exponent > 30 {
            return Err(BistrotError::ConfigValidation(
                "banquette chunk-size-exponent must be between 1 and 30".to_string(),
            ));
        }
        if let Some(token) = &self.cellar.service_token
            && token.trim().is_empty()
        {
            return Err(BistrotError::ConfigValidation(
                "cellar service-token must not be empty when set".to_string(),
            ));
        }
        if self.argenterie.port == 0 {
            return Err(BistrotError::ConfigValidation(
                "argenterie port must not be 0".to_string(),
            ));
        }
        for ext in &self.extensions {
            if ext.id.trim().is_empty() {
                return Err(BistrotError::ConfigValidation(
                    "extension id must not be empty".to_string(),
                ));
            }
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

    /// Resolves the URL for the `alhalte-argenterie` web frontend in a browser.
    pub fn argenterie_url(&self) -> String {
        format!("http://{}:{}/", self.connect_host(), self.argenterie.port)
    }

    /// Public base URL of `alhalte-maitre-d` as seen by browsers.
    ///
    /// This is the origin Google redirects back to
    /// (`{public-url}/auth/google/callback`) and must be the origin where
    /// `/auth` reaches maitre-d.
    ///
    /// Resolution order:
    /// 1. `maitre-d.public-url` when configured.
    /// 2. `https://{restaurant.domain}` when the domain is a real host (the
    ///    production reverse proxy serves argenterie and maitre-d together).
    /// 3. The argenterie URL for loopback development, so the Vite `/auth`
    ///    proxy receives the callback and the cookie lands on the UI origin.
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

        self.argenterie_url().trim_end_matches('/').to_string()
    }

    /// Browser origins `alhalte-maitre-d` should allow through CORS.
    ///
    /// Derived from the argenterie URL and the restaurant domain so the web UI
    /// keeps working in local development and behind the production domain.
    /// Unlike the old permissive default, this is an explicit allow-list.
    pub fn cors_origins(&self) -> Vec<String> {
        let mut origins = Vec::new();
        let mut push = |origin: String| {
            if !origin.is_empty() && !origins.contains(&origin) {
                origins.push(origin);
            }
        };

        push(self.argenterie_url().trim_end_matches('/').to_string());

        let domain = self.restaurant.domain.trim().trim_end_matches('/');
        if !domain.is_empty() {
            // The argenterie dev server is usually hosted on the restaurant
            // domain rather than the bind host, and production serves the UI on
            // the bare domain.
            push(format!("http://{domain}:{}", self.argenterie.port));
            push(format!("https://{domain}:{}", self.argenterie.port));
            push(format!("http://{domain}"));
            push(format!("https://{domain}"));
        }

        origins
    }

    /// Resolves the HTTP URL for `cellar`.
    pub fn cellar_url(&self) -> String {
        format!("http://{}:{}", self.connect_host(), self.cellar.port)
    }

    /// Resolves the `maitre-d` URL that `cellar` uses to verify sessions.
    ///
    /// Defaults to `http://<restaurant.host>:<maitre-d.port>`, with wildcard
    /// bind addresses (`0.0.0.0` and `::`) replaced by `127.0.0.1`.
    pub fn cellar_maitre_d_url(&self) -> String {
        self.cellar
            .maitre_d_url
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}", self.connect_host(), self.maitre_d.port))
    }

    /// Resolves the SQLite database path for `maitre-d`.
    pub fn maitre_d_db_path(&self) -> PathBuf {
        self.maitre_d
            .db_path
            .clone()
            .unwrap_or_else(|| self.restaurant.data_dir.join("maitre-d.sqlite3"))
    }

    /// Resolves the UNIX socket directory for `maitre-d`.
    pub fn maitre_d_socket_dir(&self) -> PathBuf {
        self.maitre_d
            .socket_dir
            .clone()
            .unwrap_or_else(|| self.restaurant.socket_dir.clone())
    }

    /// Resolves the storage path for `cellar` tin data.
    pub fn cellar_storage_path(&self) -> PathBuf {
        self.cellar
            .storage_path
            .clone()
            .unwrap_or_else(|| self.restaurant.data_dir.join("cellar").join("tin"))
    }

    /// Resolves the database path for `cellar`.
    pub fn cellar_db_path(&self) -> PathBuf {
        self.cellar.db_path.clone().unwrap_or_else(|| {
            self.restaurant
                .data_dir
                .join("cellar")
                .join("cellar.sqlite3")
        })
    }

    /// Resolves the UNIX socket directory for `banquette`.
    pub fn banquette_socket_dir(&self) -> PathBuf {
        self.banquette
            .socket_dir
            .clone()
            .unwrap_or_else(|| self.restaurant.socket_dir.clone())
    }

    /// Directory where decrypted secrets are materialized (`<data-dir>/secrets`).
    ///
    /// Bistrot writes the decrypted secrets there with mode `0600` and passes the
    /// file paths to the services, so secrets never appear in `ps`, systemd
    /// units, or logs (Issue #194). See [`crate::secrets::write_secret_files`].
    pub fn secrets_dir(&self) -> PathBuf {
        self.restaurant.data_dir.join("secrets")
    }

    /// Stages configured extensions into the specified target directory and generates `manifest.json`.
    ///
    /// Creates `output_dir` if it does not already exist.
    /// For each extension in `self.extensions` (with 0-based priority index):
    /// - The configured `.wasm` must exist; a missing wasm fails staging.
    /// - The wasm is copied to `<output_dir>/<ext.id>.wasm`.
    /// - An entry is added to `manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns [`BistrotError::ExtensionWasmMissing`] when a configured `.wasm`
    /// does not exist.
    /// Returns [`BistrotError`] if filesystem operations or JSON serialization fail.
    pub fn stage_extensions(
        &self,
        output_dir: impl AsRef<std::path::Path>,
    ) -> Result<Vec<ExtensionManifestEntry>> {
        self.stage_extensions_with_base(output_dir, None)
    }

    /// Stages configured extensions into the specified target directory, optionally resolving relative paths against `base_dir`.
    ///
    /// Creates `output_dir` if it does not already exist.
    /// Every configured `.wasm` must exist (at `ext.path`, or relative to `base_dir`):
    /// - A missing wasm is a `Bistrot.toml` configuration error and fails staging
    ///   before anything is copied, so no `manifest.json` entry can point at a
    ///   file that was never staged (Issue #165).
    /// - Each existing wasm is copied to `<output_dir>/<ext.id>.wasm`.
    /// - Every extension gets an entry in `manifest.json`, in configuration order
    ///   (0-based `priority`).
    ///
    /// # Errors
    ///
    /// Returns [`BistrotError::ExtensionWasmMissing`] when a configured wasm does
    /// not exist. In that case nothing is copied and `manifest.json` is not
    /// written.
    /// Returns [`BistrotError`] if filesystem operations or JSON serialization fail.
    pub fn stage_extensions_with_base(
        &self,
        output_dir: impl AsRef<std::path::Path>,
        base_dir: Option<&std::path::Path>,
    ) -> Result<Vec<ExtensionManifestEntry>> {
        let out_path = output_dir.as_ref();

        // Resolve every source before copying anything: a configuration error
        // must not leave a partially staged directory or a manifest that points
        // at a missing wasm (the browser would silently skip it).
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
                    return Err(BistrotError::ExtensionWasmMissing {
                        id: ext.id.clone(),
                        path: candidate,
                    });
                }
            } else {
                return Err(BistrotError::ExtensionWasmMissing {
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

            let entry = ExtensionManifestEntry {
                id: ext.id.clone(),
                name: ext.name.clone().unwrap_or_else(|| ext.id.clone()),
                url: format!("/extensions/{dest_wasm_name}"),
                enabled: ext.enabled,
                priority,
            };
            manifest.push(entry);
        }

        let manifest_path = out_path.join("manifest.json");
        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, json)?;

        Ok(manifest)
    }
}

/// Restaurant global settings (instance host, domain, base directories).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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

/// Configuration for `alhalte-maitre-d` (OAuth authentication and WebSocket gateway).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MaitreDConfig {
    /// Listening HTTP/WebSocket port.
    #[serde(default = "default_maitre_d_port")]
    pub port: u16,

    /// Optional explicit path to SQLite database. Defaults to `{data_dir}/maitre-d.sqlite3`.
    #[serde(default, alias = "db_path")]
    pub db_path: Option<PathBuf>,

    /// Google OAuth Client ID.
    #[serde(alias = "client_id")]
    pub client_id: String,

    /// Google OAuth Client Secret (decrypted from rops secret).
    #[serde(alias = "client_secret")]
    pub client_secret: String,

    /// Instance salt used for encrypting Google account sub.
    pub salt: String,

    /// Optional directory for banquette communication sockets. Defaults to restaurant `socket_dir`.
    #[serde(default, alias = "socket_dir")]
    pub socket_dir: Option<PathBuf>,

    /// Optional path to `alhalte-banquette` binary.
    #[serde(default, alias = "banquette_bin")]
    pub banquette_bin: Option<PathBuf>,

    /// Optional explicit URL for the cellar storage service. Defaults to cellar port on restaurant host.
    #[serde(default, alias = "cellar_url")]
    pub cellar_url: Option<String>,

    /// Optional public base URL of `maitre-d` as seen by browsers.
    ///
    /// Google OAuth redirects back to `{public-url}/auth/google/callback`. When
    /// omitted it is derived from `restaurant.domain` (production) or the
    /// argenterie URL (loopback development); see
    /// [`BistrotConfig::maitre_d_public_url`].
    #[serde(default, alias = "public_url")]
    pub public_url: Option<String>,

    /// Enables `maitre-d --debug-login` (Issue #150).
    ///
    /// Local development only. Debug login accepts an account name alone, keeps
    /// accounts and sessions in `<data-dir>/maitre-d-debug.sqlite3` (never the
    /// normal database), and spawns banquette with `--no-persist`, so no room
    /// state reaches the shared cellar. Never enable it on a public instance.
    ///
    /// `generate_maitre_d_args` emits `--debug-login` when this is true.
    #[serde(default, alias = "debug_login")]
    pub debug_login: bool,
}

/// Configuration for `alhalte-cellar` (file storage and tin deduplication).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CellarConfig {
    /// Listening port for cellar HTTP API.
    #[serde(default = "default_cellar_port")]
    pub port: u16,

    /// Optional explicit path to cellar storage directory. Defaults to `{data_dir}/cellar/tin`.
    #[serde(default, alias = "storage_path")]
    pub storage_path: Option<PathBuf>,

    /// Optional explicit path to cellar SQLite database. Defaults to `{data_dir}/cellar/cellar.sqlite3`.
    #[serde(default, alias = "db_path")]
    pub db_path: Option<PathBuf>,

    /// Anti-oracle mode flag (when true, hides duplicate detection from clients to prevent oracle attacks).
    #[serde(default = "default_anti_oracle", alias = "anti_oracle")]
    pub anti_oracle: bool,

    /// Optional explicit URL for the `maitre-d` session verification endpoint.
    ///
    /// Defaults to `http://<restaurant.host>:<maitre-d.port>` when omitted.
    #[serde(default, alias = "maitre_d_url")]
    pub maitre_d_url: Option<String>,

    /// Optional UNIX socket path for the `maitre-d` session verification endpoint.
    ///
    /// When set, it takes precedence over [`Self::maitre_d_url`].
    #[serde(default, alias = "maitre_d_socket")]
    pub maitre_d_socket: Option<PathBuf>,

    /// Allows every session without contacting `maitre-d` (local development only).
    ///
    /// Defaults to `false`. Never enable this in a production configuration.
    #[serde(default, alias = "allow_all_sessions")]
    pub allow_all_sessions: bool,

    /// Internal service token shared by `maitre-d` / `banquette` / `cellar`.
    ///
    /// rops-managed secret. `maitre-d` forwards it to every spawned banquette,
    /// which sends it as `X-Alhalte-Service-Token` for room state persistence;
    /// `cellar` accepts it for internal store / unregister calls while still
    /// verifying user sessions for the public proxy. When omitted, internal
    /// room persistence fails unless a real user session is supplied.
    #[serde(default, alias = "service_token")]
    pub service_token: Option<String>,
}

/// Configuration for `alhalte-banquette` (in-memory room session processes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BanquetteConfig {
    /// Log chunk size exponent N (where 1 << N is the number of logs per chunk, default 8 = 256).
    #[serde(default = "default_chunk_size_exponent", alias = "chunk_size_exponent")]
    pub chunk_size_exponent: u8,

    /// Optional directory for room sockets. Defaults to restaurant `socket_dir`.
    #[serde(default, alias = "socket_dir")]
    pub socket_dir: Option<PathBuf>,
}

impl Default for BanquetteConfig {
    fn default() -> Self {
        Self {
            chunk_size_exponent: default_chunk_size_exponent(),
            socket_dir: None,
        }
    }
}

/// Configuration for `alhalte-argenterie` (frontend web development server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ArgenterieConfig {
    /// Listening HTTP port for argenterie frontend server.
    #[serde(default = "default_argenterie_port")]
    pub port: u16,
}

impl Default for ArgenterieConfig {
    fn default() -> Self {
        Self {
            port: default_argenterie_port(),
        }
    }
}

/// Default extension entry configured in `Bistrot.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExtensionConfig {
    /// Unique identifier for the extension (e.g. `"alhalte-dsl"`).
    pub id: String,

    /// Display name fallback if WASM self_introduction is not yet loaded.
    #[serde(default)]
    pub name: Option<String>,

    /// Path to the compiled `.wasm` binary (relative to working directory or absolute).
    pub path: PathBuf,

    /// Whether the extension is enabled by default in rooms.
    #[serde(default = "default_extension_enabled")]
    pub enabled: bool,
}

/// Extension manifest entry written into `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExtensionManifestEntry {
    /// Unique identifier for the extension (e.g. `"alhalte-dsl"`).
    pub id: String,

    /// Display name of the extension.
    pub name: String,

    /// URL to download the extension `.wasm` file (e.g. `"/extensions/alhalte-dsl.wasm"`).
    pub url: String,

    /// Whether the extension is enabled by default in rooms.
    pub enabled: bool,

    /// Execution priority order index.
    pub priority: usize,
}
