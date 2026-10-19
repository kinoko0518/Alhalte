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

    // The token comes from a 0600 file when maitre-d forwards one; argv and the
    // environment stay backward compatible (Issue #194).
    let service_token = args.resolve_service_token()?;
    let mut cellar = HttpCellarSink::new(args.cellar_url);
    match service_token {
        Some(token) => cellar = cellar.with_service_token(token),
        None => tracing::warn!(
            "No --service-token-file configured: room state persistence will fail unless cellar \
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
    };

    // Flush the final `RoomData` to cellar when the service manager or an
    // operator stops the room with SIGTERM / SIGINT. The graceful path
    // (`run_until_shutdown`) persists once more and reports `RoomPersisted`.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        match wait_for_termination_signal().await {
            Ok(()) => {
                tracing::info!(
                    "SIGTERM/SIGINT received; persisting the final room state and shutting down"
                );
                let _ = shutdown_tx.send(true);
            }
            Err(e) => {
                // Keep the sender alive so the server does not mistake this
                // failure for a shutdown request; the process keeps running
                // with the platform's default signal behaviour.
                tracing::error!("Failed to install SIGTERM/SIGINT handlers: {e}");
                std::future::pending::<()>().await;
            }
        }
    });

    let server = BanquetteServer::new(config, room, cellar);
    server.run_until_shutdown(shutdown_rx).await?;

    Ok(())
}

/// Resolves when the process receives `SIGTERM` or `SIGINT`.
///
/// Used by the standalone binary only; tests drive [`BanquetteServer::run_until_shutdown`]
/// directly with the same shutdown channel.
async fn wait_for_termination_signal() -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }

    Ok(())
}
