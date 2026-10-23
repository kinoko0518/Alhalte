//! Command-line binary for launching `alhalte-banquette`.

use std::sync::Arc;
use std::time::Duration;

use alhalte_banquette::cellar_sink::{CellarSink, HttpCellarSink};
use alhalte_banquette::cli::parse_cli_args;
use alhalte_banquette::room::Room;
use alhalte_banquette::server::{BanquetteServer, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = parse_cli_args();
    tracing::info!(
        "Starting alhalte-banquette room_id={:016x} admin={} socket={:?} pipe={:?}",
        args.room_id,
        args.admin,
        args.socket,
        args.pipe_name
    );

    // The token comes from a 0600 file when maitre-d forwards one; argv and
    // the environment stay backward compatible (Issue #194).
    let service_token = args.resolve_service_token()?;
    let mut http_cellar = HttpCellarSink::new(args.cellar_url);
    match service_token {
        Some(token) => http_cellar = http_cellar.with_service_token(token),
        // Head alias writes are service-token only, so a token is now
        // required for persistence even when a user session could authorize
        // the store.
        None => tracing::warn!(
            "No --service-token-file configured: room state persistence will fail because the \
             room head alias is written with the cellar service token"
        ),
    }
    let cellar: Arc<dyn CellarSink> = Arc::new(http_cellar);

    // The room restores itself from cellar: the `room/<room_id>/head` alias is
    // the only source of a persisted head, so nothing is passed on the command
    // line. A set alias that cannot be restored aborts startup instead of
    // silently starting an empty room; no alias starts a brand-new room.
    let room = Room::load_or_create(
        cellar.as_ref(),
        args.room_id,
        args.admin,
        args.chunk_size_exponent,
    )
    .await?;

    let config = ServerConfig {
        socket_path: args.socket,
        pipe_name: args.pipe_name,
        heartbeat_interval: Duration::from_secs(args.heartbeat_interval_secs),
    };

    // Flush the final `RoomData` to cellar when the service manager or an
    // operator stops the room with SIGTERM / SIGINT. The graceful path
    // (`run_until_shutdown`) persists once more and moves the head alias.
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

/// Resolves when the process receives `SIGTERM` / `SIGINT` (Unix) or Ctrl+C
/// (Windows).
///
/// Used by the standalone binary only; tests drive [`BanquetteServer::run_until_shutdown`]
/// directly with the same shutdown channel.
#[cfg(unix)]
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

#[cfg(not(unix))]
async fn wait_for_termination_signal() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}
