//! CLI argument parsing definitions for `alhalte-cellar`.

use clap::Parser;
use std::path::PathBuf;

use crate::secret_file::{SecretFileError, resolve_secret};

/// CLI arguments for `alhalte-cellar`.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(
    name = "alhalte-cellar",
    about = "Deduplicated tin storage service and garbage collector for Alhalte",
    version
)]
pub struct CliArgs {
    /// Host address to bind to.
    #[arg(long, default_value = "127.0.0.1", env = "ALHALTE_CELLAR_HOST")]
    pub host: String,

    /// Port to listen on.
    #[arg(long, default_value_t = 8081, env = "ALHALTE_CELLAR_PORT")]
    pub port: u16,

    /// Directory path for storing tin entity files.
    #[arg(
        long,
        default_value = "./data/cellar/tin",
        env = "ALHALTE_CELLAR_STORAGE_PATH"
    )]
    pub storage_path: PathBuf,

    /// SQLite database path for indexing tin references.
    #[arg(
        long,
        default_value = "./data/cellar/cellar.sqlite3",
        env = "ALHALTE_CELLAR_DB_PATH"
    )]
    pub db_path: PathBuf,

    /// Enables anti-oracle mode to prevent existence-probing attacks.
    ///
    /// Accepts `--anti-oracle`, `--anti-oracle true`, or `--anti-oracle false`.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", default_value_t = false)]
    pub anti_oracle: bool,

    /// URL for the `alhalte-maitre-d` service (e.g. `http://127.0.0.1:8080`).
    #[arg(long, env = "ALHALTE_MAITRE_D_URL")]
    pub maitre_d_url: Option<String>,

    /// Unix domain socket path for `alhalte-maitre-d`.
    #[arg(long, env = "ALHALTE_MAITRE_D_SOCKET")]
    pub maitre_d_socket: Option<PathBuf>,

    /// Use an in-memory session verifier that allows all requests (development/test only).
    #[arg(long, default_value_t = false)]
    pub allow_all_sessions: bool,

    /// Internal service token accepted from trusted services (`alhalte-banquette`).
    ///
    /// Requests carrying this token in the `X-Alhalte-Service-Token` header may
    /// store and unregister tins without a user session. User session
    /// verification is unaffected. The value is never logged.
    ///
    /// Prefer `--service-token-file`: an inline value is visible in `ps`.
    #[arg(long, env = "ALHALTE_SERVICE_TOKEN", hide_env_values = true)]
    pub service_token: Option<String>,

    /// File containing the internal service token (mode 0600).
    ///
    /// alhalte-restaurant writes the decrypted secret to `<data-dir>/secrets/` and passes
    /// this path, so the token never appears in `ps` or in a systemd unit
    /// (Issue #194). Takes precedence over `--service-token`.
    #[arg(long, env = "ALHALTE_SERVICE_TOKEN_FILE")]
    pub service_token_file: Option<PathBuf>,

    /// Periodic garbage collection interval in seconds.
    #[arg(long, default_value_t = 3600)]
    pub gc_interval_secs: u64,
}

impl CliArgs {
    /// Resolves the internal service token from the file or the inline value.
    ///
    /// The file (`--service-token-file` / `ALHALTE_SERVICE_TOKEN_FILE`) wins, so
    /// the injected secret cannot be shadowed by a stale environment value.
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

/// Helper function to parse CLI arguments from an iterator of string slices (useful for tests).
pub fn parse_cli_args<I, T>(itr: I) -> Result<CliArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    CliArgs::try_parse_from(itr)
}

#[cfg(test)]
mod tests {
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

    // spec: alhalte-cellar/cli 0007 the token is read from a 0600 file and the
    // file wins over an inline value
    #[test]
    fn resolves_service_token_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("service-token");
        write_secret(&path, "file-token");

        let args = CliArgs::try_parse_from([
            "alhalte-cellar",
            "--service-token",
            "inline-token",
            "--service-token-file",
            path.to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(
            args.resolve_service_token().unwrap().as_deref(),
            Some("file-token")
        );

        // Without a file the inline value keeps working.
        let args =
            CliArgs::try_parse_from(["alhalte-cellar", "--service-token", "inline-token"]).unwrap();
        assert_eq!(
            args.resolve_service_token().unwrap().as_deref(),
            Some("inline-token")
        );

        // A group/other-readable file is rejected.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let args = CliArgs::try_parse_from([
            "alhalte-cellar",
            "--service-token-file",
            path.to_str().unwrap(),
        ])
        .unwrap();
        assert!(args.resolve_service_token().is_err());
    }
}
