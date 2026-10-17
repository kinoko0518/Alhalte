//! Command-line interface arguments for `alhalte-banquette`.

use std::path::PathBuf;

use clap::Parser;

/// Command-line arguments for the `alhalte-banquette` room daemon.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "alhalte-banquette",
    about = "Session room process managing scenes, logs, and state relay for Alhalte",
    version
)]
pub struct CliArgs {
    /// 64-bit room identifier.
    #[arg(long, env = "ALHALTE_ROOM_ID")]
    pub room_id: u64,

    /// Initial room administrator account ID.
    #[arg(long, alias = "owner", env = "ALHALTE_ADMIN_ACCOUNT")]
    pub admin: u64,

    /// UNIX domain socket path for communication with `alhalte-maitre-d`.
    #[arg(
        long,
        default_value = "/tmp/alhalte-banquette.sock",
        env = "ALHALTE_SOCKET_PATH"
    )]
    pub socket: PathBuf,

    /// Base HTTP URL of the `alhalte-cellar` content storage service.
    #[arg(
        long,
        default_value = "http://127.0.0.1:8080",
        env = "ALHALTE_CELLAR_URL"
    )]
    pub cellar_url: String,

    /// Log chunk size exponent $N$. One chunk holds $1 \ll N$ entries (default 8 = 256 entries).
    #[arg(
        short = 'n',
        long,
        default_value = "8",
        env = "ALHALTE_CHUNK_SIZE_EXPONENT"
    )]
    pub chunk_size_exponent: u8,

    /// Periodic heartbeat interval in seconds (default 240 seconds = 4 minutes).
    #[arg(long, default_value = "240", env = "ALHALTE_HEARTBEAT_INTERVAL_SECS")]
    pub heartbeat_interval_secs: u64,
}

/// Parses CLI arguments from `std::env::args_os()`.
pub fn parse_cli_args() -> CliArgs {
    CliArgs::parse()
}
