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
        "Starting alhalte-banquette room_id={:016x} admin={} socket={:?}",
        args.room_id,
        args.admin,
        args.socket
    );

    let mut cellar = HttpCellarSink::new(args.cellar_url);
    match args.service_token.as_deref() {
        Some(token) => cellar = cellar.with_service_token(token),
        None => tracing::warn!(
            "No --service-token configured: room state persistence will fail unless cellar \
             accepts the write through a user session"
        ),
    }
    let cellar = Arc::new(cellar);

    // A persisted head must be restored; a failure here aborts startup instead of
    // silently starting an empty room.
    let room = match args.room_tin {
        Some(tin_id) => {
            tracing::info!(
                "Restoring room {:016x} from persisted tin {}",
                args.room_id,
                alhalte_cellar::format_tin_id(&tin_id)
            );
            Room::load_from_cellar(cellar.as_ref(), &tin_id, args.room_id)
                .await
                .map_err(|e| {
                    format!(
                        "failed to restore persisted room {:016x} from tin {}: {e}",
                        args.room_id,
                        alhalte_cellar::format_tin_id(&tin_id)
                    )
                })?
        }
        None => Room::new(args.room_id, args.admin, args.chunk_size_exponent)?,
    };

    let config = ServerConfig {
        socket_path: args.socket,
        heartbeat_interval: Duration::from_secs(args.heartbeat_interval_secs),
        admin_account: args.admin,
    };

    let server = BanquetteServer::new(config, room, cellar);
    server.run().await?;

    Ok(())
}
