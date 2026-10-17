//! Command-line interface arguments for `alhalte-maitre-d`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::Parser;

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

    /// Google OAuth Client ID.
    #[arg(long, default_value = "", env = "ALHALTE_CLIENT_ID")]
    pub client_id: String,

    /// Google OAuth Client Secret.
    #[arg(long, default_value = "", env = "ALHALTE_CLIENT_SECRET")]
    pub client_secret: String,

    /// Instance-specific salt for deterministic Google `sub` encryption.
    #[arg(long, default_value = "alhalte-salt", env = "ALHALTE_SALT")]
    pub salt: String,

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

    /// Optional UNIX domain socket path to bind for HTTP/API requests.
    #[arg(long, env = "ALHALTE_UNIX_SOCKET")]
    pub unix_socket: Option<PathBuf>,
}

/// Parses CLI arguments from an arbitrary iterator of strings (useful for unit tests).
pub fn parse_cli_args_from<I, T>(itr: I) -> Result<CliArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    CliArgs::try_parse_from(itr)
}

/// Parses CLI arguments from `std::env::args_os()`.
pub fn parse_cli_args() -> CliArgs {
    CliArgs::parse()
}
