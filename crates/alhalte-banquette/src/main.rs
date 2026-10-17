//! Command-line binary for launching `alhalte-banquette`.

use std::sync::Arc;
use std::time::Duration;

use alhalte_banquette::cellar_sink::HttpCellarSink;
use alhalte_banquette::cli::parse_cli_args;
use alhalte_banquette::room::Room;
use alhalte_banquette::server::{BanquetteServer, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = parse_cli_args();
    tracing::info!(
        "Starting alhalte-banquette room_id={} admin={} socket={:?}",
        args.room_id,
        args.admin,
        args.socket
    );

    let cellar = Arc::new(HttpCellarSink::new(args.cellar_url));
    let room = Room::new(args.room_id, args.admin, args.chunk_size_exponent)?;

    let config = ServerConfig {
        socket_path: args.socket,
        heartbeat_interval: Duration::from_secs(args.heartbeat_interval_secs),
        admin_account: args.admin,
        system_session_id: 1,
    };

    let server = BanquetteServer::new(config, room, cellar);
    server.run().await?;

    Ok(())
}
