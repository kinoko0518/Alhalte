//! CLI argument parsing definitions for `alhalte-cellar`.

use clap::Parser;
use std::path::PathBuf;

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

    /// Periodic garbage collection interval in seconds.
    #[arg(long, default_value_t = 3600)]
    pub gc_interval_secs: u64,
}

/// Helper function to parse CLI arguments from an iterator of string slices (useful for tests).
pub fn parse_cli_args<I, T>(itr: I) -> Result<CliArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    CliArgs::try_parse_from(itr)
}
