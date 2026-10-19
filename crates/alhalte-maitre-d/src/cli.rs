//! Command-line interface arguments for `alhalte-maitre-d`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::Parser;

use crate::secret_file::{SecretFileError, resolve_secret};

/// File name of the isolated debug-login database created next to `--db-path`.
///
/// `maitre-d --debug-login` never opens the normal database; it opens
/// `{db-path のディレクトリ}/maitre-d-debug.sqlite3` instead (Issue #150).
pub const DEBUG_DB_FILE_NAME: &str = "maitre-d-debug.sqlite3";

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

    /// Enables the isolated debug login (Issue #150).
    ///
    /// While this flag is set, `GET /auth/login` redirects to
    /// `/auth/debug/login`, where an account name alone signs in (creating the
    /// account on first use). Accounts and sessions live in a separate SQLite
    /// file — this process never opens or touches the normal database — and
    /// spawned banquette processes run with `--no-persist`, so no room state is
    /// written to the shared cellar.
    ///
    /// Intended for local development only. Never enable it on a public
    /// instance: anyone who can reach the port can create an account.
    ///
    /// `ALHALTE_DEBUG_LOGIN` is a value switch: set it to `true` (or `false`) to
    /// enable / disable the flag, or omit it.
    #[arg(long, env = "ALHALTE_DEBUG_LOGIN")]
    pub debug_login: bool,

    /// SQLite file used while `--debug-login` is active.
    ///
    /// Defaults to [`DEBUG_DB_FILE_NAME`] next to `--db-path`. The same schema is
    /// applied, so the file is a normal account/session database, but it is a
    /// different file: switching `--debug-login` on and off never mixes debug
    /// accounts with real ones.
    #[arg(long, env = "ALHALTE_DEBUG_DB_PATH")]
    pub debug_db_path: Option<PathBuf>,

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
    /// Bistrot writes the decrypted secret to `<data-dir>/secrets/` and passes
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
    /// reverse-proxied argenterie origin or the restaurant domain). Bistrot
    /// derives it from `maitre-d.public-url`, the restaurant domain, or the
    /// argenterie URL.
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
    /// the restaurant's argenterie URL / domain here.
    #[arg(
        long = "cors-origin",
        env = "ALHALTE_CORS_ORIGINS",
        value_delimiter = ',',
        value_name = "ORIGIN"
    )]
    pub cors_origins: Vec<String>,
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
    /// Without `--debug-login` it is `--db-path`. With `--debug-login` it is
    /// `--debug-db-path`, or [`DEBUG_DB_FILE_NAME`] in the `--db-path` directory,
    /// so the normal database is never opened while debug login is active.
    ///
    /// spec: alhalte-maitre-d/debug-login 0001 debug login uses a separate SQLite file
    pub fn active_db_path(&self) -> PathBuf {
        if !self.debug_login {
            return self.db_path.clone();
        }
        if let Some(path) = &self.debug_db_path {
            return path.clone();
        }
        match self.db_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(DEBUG_DB_FILE_NAME),
            _ => PathBuf::from(DEBUG_DB_FILE_NAME),
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
mod debug_login_tests {
    use super::*;

    // spec: alhalte-maitre-d/debug-login 0001 debug login uses a separate SQLite file
    #[test]
    fn debug_login_selects_a_separate_database_file() {
        // Normal mode keeps the configured path.
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--db-path",
            "/var/lib/alhalte/maitre-d.sqlite3",
        ])
        .unwrap();
        assert!(!args.debug_login);
        assert_eq!(args.debug_db_path, None);
        assert_eq!(
            args.active_db_path(),
            PathBuf::from("/var/lib/alhalte/maitre-d.sqlite3")
        );

        // Debug mode defaults to a sibling file and never the normal one.
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--db-path",
            "/var/lib/alhalte/maitre-d.sqlite3",
            "--debug-login",
        ])
        .unwrap();
        assert!(args.debug_login);
        assert_eq!(
            args.active_db_path(),
            PathBuf::from("/var/lib/alhalte/maitre-d-debug.sqlite3")
        );
        assert_ne!(args.active_db_path(), args.db_path);

        // An explicit debug path wins.
        let args = CliArgs::try_parse_from([
            "alhalte-maitre-d",
            "--db-path",
            "/var/lib/alhalte/maitre-d.sqlite3",
            "--debug-login",
            "--debug-db-path",
            "/tmp/alhalte-debug.sqlite3",
        ])
        .unwrap();
        assert_eq!(
            args.active_db_path(),
            PathBuf::from("/tmp/alhalte-debug.sqlite3")
        );

        // A bare relative --db-path keeps the debug file relative.
        let args = CliArgs::try_parse_from(["alhalte-maitre-d", "--debug-login"]).unwrap();
        assert_eq!(args.active_db_path(), PathBuf::from(DEBUG_DB_FILE_NAME));
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
