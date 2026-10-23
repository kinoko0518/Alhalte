//! Command-line interface arguments for `alhalte-maitre-d`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::Parser;

use crate::secret_file::{SecretFileError, resolve_secret};

/// File name of the isolated local-login database created next to `--db-path`.
///
/// `maitre-d --local-login` never opens the normal (Google) database; it opens
/// `{db-path のディレクトリ}/maitre-d-local.sqlite3` instead.
pub const LOCAL_DB_FILE_NAME: &str = "maitre-d-local.sqlite3";

/// Command-line arguments for the `alhalte-maitre-d` service.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "alhalte-maitre-d",
    about = "Authentication, SQLite account/session management, and WebSocket room gateway for Alhalte",
    version
)]
pub struct CliArgs {
    /// Host address to bind the HTTP/WebSocket server to.
    #[arg(long, default_value = "127.0.0.1", env = "ALHALTE_HOST")]
    pub host: String,

    /// Port to listen on.
    #[arg(long, default_value = "8080", env = "ALHALTE_PORT")]
    pub port: u16,

    /// Path to SQLite database file.
    #[arg(long, default_value = "maitre-d.sqlite3", env = "ALHALTE_DB_PATH")]
    pub db_path: PathBuf,

    /// Enables the local password login.
    ///
    /// While this flag is set, `GET /auth/login` redirects to the SPA `/login`
    /// route, where an account name and password sign in. Accounts, sessions,
    /// and the room registry live in a separate SQLite file
    /// ([`LOCAL_DB_FILE_NAME`]); this process never opens the normal Google
    /// database. Room state is persisted to cellar normally.
    ///
    /// Intended for local / trusted-LAN instances without Google OAuth. The
    /// login is HTTP-only, so do not expose it to the public internet.
    ///
    /// `ALHALTE_LOCAL_LOGIN` is a value switch: set it to `true` (or `false`) to
    /// enable / disable the flag, or omit it.
    #[arg(long, env = "ALHALTE_LOCAL_LOGIN")]
    pub local_login: bool,

    /// SQLite file used while `--local-login` is active.
    ///
    /// Defaults to [`LOCAL_DB_FILE_NAME`] next to `--db-path`. The same schema is
    /// applied, so the file is a normal account/session database, but it is a
    /// different file: local accounts never mix with Google accounts.
    #[arg(long, env = "ALHALTE_LOCAL_DB_PATH")]
    pub local_db_path: Option<PathBuf>,

    /// Directory containing the built SPA (`index.html`, `assets/`, ...).
    ///
    /// When set, maitre-d serves the web UI itself (same origin as `/api` and
    /// `/auth`). The Windows package sets this to the bundled `web/` directory.
    #[arg(long, env = "ALHALTE_WEB_ROOT")]
    pub web_root: Option<PathBuf>,

    /// Heartbeat interval forwarded to every spawned banquette process.
    ///
    /// The interval controls how often a room is persisted to cellar. The
    /// Windows package uses 60 seconds so a forced shutdown loses at most about
    /// one minute of room state.
    #[arg(
        long,
        default_value_t = 240,
        env = "ALHALTE_BANQUETTE_HEARTBEAT_INTERVAL_SECS"
    )]
    pub banquette_heartbeat_interval_secs: u64,

    /// Google OAuth Client ID.
    #[arg(long, default_value = "", env = "ALHALTE_CLIENT_ID")]
    pub client_id: String,

    /// Google OAuth Client Secret.
    ///
    /// Prefer `--client-secret-file`: an inline value is visible in `ps`.
    #[arg(
        long,
        default_value = "",
        env = "ALHALTE_CLIENT_SECRET",
        hide_env_values = true
    )]
    pub client_secret: String,

    /// File containing the Google OAuth client secret (mode 0600).
    ///
    /// alhalte-restaurant writes the decrypted secret to `<data-dir>/secrets/` and passes
    /// this path, so the secret never appears in `ps` or in a systemd unit
    /// (Issue #194). Takes precedence over `--client-secret`.
    #[arg(long, env = "ALHALTE_CLIENT_SECRET_FILE")]
    pub client_secret_file: Option<PathBuf>,

    /// Instance-specific salt for deterministic Google `sub` encryption.
    #[arg(long, default_value = "alhalte-salt", env = "ALHALTE_SALT")]
    pub salt: String,

    /// Public base URL of this service as seen by browsers.
    ///
    /// Google OAuth redirects back to `{public-url}/auth/google/callback`, so
    /// this value is the single fixed redirect URI registered with Google. It
    /// must be the origin where `/auth` reaches `maitre-d` (normally the
    /// reverse-proxied UI origin or the restaurant domain). alhalte-restaurant
    /// derives it from `maitre-d.public-url`, the restaurant domain, or the
    /// maitre-d URL.
    #[arg(
        long,
        default_value = "http://127.0.0.1:8080",
        env = "ALHALTE_PUBLIC_URL"
    )]
    pub public_url: String,

    /// Directory for banquette UNIX domain sockets.
    #[arg(long, default_value = "/run/alhalte", env = "ALHALTE_SOCKET_DIR")]
    pub socket_dir: PathBuf,

    /// Path to `alhalte-banquette` binary when spawning external child processes.
    #[arg(
        long,
        default_value = "alhalte-banquette",
        env = "ALHALTE_BANQUETTE_BIN"
    )]
    pub banquette_bin: PathBuf,

    /// URL of the `alhalte-cellar` storage service passed to spawned banquette instances.
    #[arg(
        long,
        default_value = "http://127.0.0.1:8080",
        env = "ALHALTE_CELLAR_URL"
    )]
    pub cellar_url: String,

    /// Log chunk size exponent passed to spawned banquette instances (`1..=30`).
    #[arg(long, default_value_t = 8, env = "ALHALTE_CHUNK_SIZE_EXPONENT")]
    pub chunk_size_exponent: u8,

    /// Optional UNIX domain socket path to bind for HTTP/API requests.
    #[arg(long, env = "ALHALTE_UNIX_SOCKET")]
    pub unix_socket: Option<PathBuf>,

    /// Internal cellar service token forwarded to every spawned banquette.
    ///
    /// Banquette sends it as `X-Alhalte-Service-Token` so room state persistence
    /// is authorized without impersonating a user session. `alhalte-cellar`
    /// must be configured with the same token. The value is never logged.
    ///
    /// Prefer `--service-token-file`: an inline value is visible in `ps`.
    #[arg(long, env = "ALHALTE_SERVICE_TOKEN", hide_env_values = true)]
    pub service_token: Option<String>,

    /// File containing the internal cellar service token (mode 0600).
    ///
    /// The path is forwarded to every spawned banquette as
    /// `--service-token-file`, so the token never appears in `ps` or in a
    /// systemd unit (Issue #194). Takes precedence over `--service-token`.
    #[arg(long, env = "ALHALTE_SERVICE_TOKEN_FILE")]
    pub service_token_file: Option<PathBuf>,

    /// Browser origin allowed by CORS. Repeat the flag or use a comma-separated
    /// value / `ALHALTE_CORS_ORIGINS`.
    ///
    /// When omitted, only the local development origins
    /// (`http://127.0.0.1:5173`, `http://localhost:5173`) are accepted. CORS
    /// never falls back to `permissive()`; production deployments should pass
    /// the restaurant's maitre-d public URL / domain here.
    #[arg(
        long = "cors-origin",
        env = "ALHALTE_CORS_ORIGINS",
        value_delimiter = ',',
        value_name = "ORIGIN"
    )]
    pub cors_origins: Vec<String>,

    /// Private-network LAN: also accept `http://<private IPv4>:<PORT>` browser
    /// origins for CORS and OAuth `return_to`.
    ///
    /// The flag takes the maitre-d port and is repeatable (comma-separated /
    /// `ALHALTE_ALLOW_LAN_ORIGIN_PORTS`). alhalte-restaurant emits it only when
    /// `restaurant.host = "0.0.0.0"` in the bundled local-development configuration
    /// (Issue #229), so phones on the same Ethernet can sign in without making
    /// CORS permissive:
    /// - only `http://` (never `https://`) origins are accepted,
    /// - the host must be an IP literal in a private range (RFC 1918 or
    ///   loopback), never a hostname or a public address,
    /// - the port must be one of the listed ports.
    ///
    /// Never enable this on a public instance.
    #[arg(
        long = "allow-lan-origin-port",
        env = "ALHALTE_ALLOW_LAN_ORIGIN_PORTS",
        value_delimiter = ',',
        value_name = "PORT"
    )]
    pub allow_lan_origin_ports: Vec<u16>,
}

/// Parses CLI arguments from an arbitrary iterator of strings (useful for unit tests).
pub fn parse_cli_args_from<I, T>(itr: I) -> Result<CliArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    CliArgs::try_parse_from(itr)
}

impl CliArgs {
    /// Returns the SQLite file this process actually opens.
    ///
    /// Without `--local-login` it is `--db-path`. With `--local-login` it is
    /// `--local-db-path`, or [`LOCAL_DB_FILE_NAME`] in the `--db-path` directory,
    /// so the normal Google database is never opened while local login is active.
    ///
    /// spec: alhalte-maitre-d/local-login 0006 local login uses a separate SQLite file
    pub fn active_db_path(&self) -> PathBuf {
        if !self.local_login {
            return self.db_path.clone();
        }
        if let Some(path) = &self.local_db_path {
            return path.clone();
        }
        match self.db_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(LOCAL_DB_FILE_NAME),
            _ => PathBuf::from(LOCAL_DB_FILE_NAME),
        }
    }

    /// Resolves the Google OAuth client secret from the file or the inline value.
    ///
    /// The file (`--client-secret-file` / `ALHALTE_CLIENT_SECRET_FILE`) wins. An
    /// unset secret stays empty, matching the previous default.
    ///
    /// # Errors
    ///
    /// Returns [`SecretFileError`] when the file is not owner-only, empty, or
    /// unreadable.
    pub fn resolve_client_secret(&self) -> Result<String, SecretFileError> {
        Ok(resolve_secret(
            self.client_secret_file.as_deref(),
            Some(&self.client_secret),
        )?
        .unwrap_or_default())
    }

    /// Resolves the internal service token from the file or the inline value.
    ///
    /// # Errors
    ///
    /// Returns [`SecretFileError`] when the file is not owner-only, empty, or
    /// unreadable.
    pub fn resolve_service_token(&self) -> Result<Option<String>, SecretFileError> {
        resolve_secret(
            self.service_token_file.as_deref(),
            self.service_token.as_deref(),
        )
    }
}

/// Parses CLI arguments from `std::env::args_os()`.
pub fn parse_cli_args() -> CliArgs {
    CliArgs::parse()
}

#[cfg(test)]
mod local_login_tests {
    use super::*;

    // spec: alhalte-maitre-d/local-login 0006 local login uses a separate SQLite file
    #[test]
    fn local_login_selects_a_separate_database_file() {
        // Normal mode keeps the configured path.
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--db-path",
            "/var/lib/alhalte/maitre-d.sqlite3",
        ])
        .unwrap();
        assert!(!args.local_login);
        assert_eq!(args.local_db_path, None);
        assert_eq!(
            args.active_db_path(),
            PathBuf::from("/var/lib/alhalte/maitre-d.sqlite3")
        );

        // Local mode defaults to a sibling file and never the normal one.
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--db-path",
            "/var/lib/alhalte/maitre-d.sqlite3",
            "--local-login",
        ])
        .unwrap();
        assert!(args.local_login);
        assert_eq!(
            args.active_db_path(),
            PathBuf::from("/var/lib/alhalte/maitre-d-local.sqlite3")
        );
        assert_ne!(args.active_db_path(), args.db_path);

        // An explicit local path wins.
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--db-path",
            "/var/lib/alhalte/maitre-d.sqlite3",
            "--local-login",
            "--local-db-path",
            "/tmp/alhalte-local.sqlite3",
        ])
        .unwrap();
        assert_eq!(
            args.active_db_path(),
            PathBuf::from("/tmp/alhalte-local.sqlite3")
        );

        // A bare relative --db-path keeps the local file relative.
        let args = CliArgs::try_parse_from(["alhalte-maitre-d", "--local-login"]).unwrap();
        assert_eq!(args.active_db_path(), PathBuf::from(LOCAL_DB_FILE_NAME));
    }
}

#[cfg(test)]
mod secret_file_tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fn write_secret(path: &std::path::Path, value: &str) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(value.as_bytes()).unwrap();
        file.write_all(b"\n").unwrap();
    }

    // spec: alhalte-maitre-d/cli 0007 the OAuth secret and service token are read
    // from 0600 files, never from argv
    #[test]
    fn resolves_secrets_from_files() {
        let dir = tempfile::tempdir().unwrap();
        let client_secret = dir.path().join("client-secret");
        let service_token = dir.path().join("service-token");
        write_secret(&client_secret, "file-oauth-secret");
        write_secret(&service_token, "file-token");

        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--client-secret",
            "inline-oauth-secret",
            "--client-secret-file",
            client_secret.to_str().unwrap(),
            "--service-token",
            "inline-token",
            "--service-token-file",
            service_token.to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(args.resolve_client_secret().unwrap(), "file-oauth-secret");
        assert_eq!(
            args.resolve_service_token().unwrap().as_deref(),
            Some("file-token")
        );

        // An unset OAuth secret stays empty, matching the previous default.
        let args = CliArgs::try_parse_from(["alhalte-maitre-d"]).unwrap();
        assert_eq!(args.resolve_client_secret().unwrap(), "");

        // A group/other-readable secret file is rejected.
        std::fs::set_permissions(&client_secret, std::fs::Permissions::from_mode(0o604)).unwrap();
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--client-secret-file",
            client_secret.to_str().unwrap(),
        ])
        .unwrap();
        assert!(args.resolve_client_secret().is_err());
    }
}
