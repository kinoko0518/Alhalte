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

fn default_anti_oracle() -> bool {
    false
}

fn default_chunk_size_exponent() -> u8 {
    DEFAULT_CHUNK_SIZE_EXPONENT
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
        if self.banquette.chunk_size_exponent == 0 || self.banquette.chunk_size_exponent > 30 {
            return Err(BistrotError::ConfigValidation(
                "banquette chunk-size-exponent must be between 1 and 30".to_string(),
            ));
        }
        Ok(())
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
