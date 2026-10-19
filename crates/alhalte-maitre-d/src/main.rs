//! Binary entrypoint for `alhalte-maitre-d`.

use std::sync::Arc;

use alhalte_maitre_d::server::CellarProxy;
use alhalte_maitre_d::{
    AppState, Database, GoogleOAuthProvider, ProcessBanquetteSpawner, RoomBridgeManager,
    create_router, parse_cli_args, spawn_session_cleanup_task,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,alhalte_maitre_d=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = parse_cli_args();
    tracing::info!(
        "Starting alhalte-maitre-d on {}:{} (db: {:?})",
        args.host,
        args.port,
        args.db_path
    );

    let db = Database::open(&args.db_path)?;
    tracing::info!("SQLite database opened at {:?}", args.db_path);

    // Spawn periodic 4:00 AM session cleanup task
    spawn_session_cleanup_task(db.clone());

    // Secrets are read from 0600 files (or the environment) so they never appear
    // in `ps` or in a systemd unit (Issue #194).
    let client_secret = args.resolve_client_secret()?;
    let service_token = args.resolve_service_token()?;

    let oauth = Arc::new(GoogleOAuthProvider::new(&args.client_id, &client_secret));

    let mut spawner = ProcessBanquetteSpawner::new(
        &args.banquette_bin,
        &args.socket_dir,
        &args.cellar_url,
        args.chunk_size_exponent,
    );
    match (service_token.as_deref(), args.service_token_file.as_deref()) {
        // Forward the injected file path so the child never holds the value in
        // its argument list.
        (Some(_), Some(path)) => {
            spawner = spawner.with_service_token_file(path);
        }
        (Some(token), None) => {
            spawner = spawner.with_service_token(token);
        }
        (None, _) => tracing::warn!(
            "No --service-token-file configured: spawned banquette processes cannot persist room \
             state unless cellar accepts the write through a user session"
        ),
    }
    let spawner = Arc::new(spawner);

    let room_manager = RoomBridgeManager::new(spawner, db.clone());

    let mut state = AppState::new(db, args.salt, oauth, room_manager)
        .with_public_url(&args.public_url)
        .with_cellar_proxy(CellarProxy::new(&args.cellar_url)?);
    tracing::info!("Google OAuth redirect URI: {}", state.oauth_redirect_uri());
    if !args.cors_origins.is_empty() {
        tracing::info!("CORS allowed origins: {}", args.cors_origins.join(", "));
        state = state.with_allowed_origins(args.cors_origins);
    }
    let app = create_router(state);

    // If an optional UNIX domain socket is specified, also serve on it
    if let Some(unix_path) = &args.unix_socket {
        if unix_path.exists() {
            let _ = tokio::fs::remove_file(unix_path).await;
        }
        if let Some(parent) = unix_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let unix_listener = tokio::net::UnixListener::bind(unix_path)?;
        tracing::info!("Serving HTTP API on UNIX domain socket: {:?}", unix_path);

        let unix_app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = axum::serve(unix_listener, unix_app).await {
                tracing::error!("UNIX socket server error: {e:?}");
            }
        });
    }

    let addr = format!("{}:{}", args.host, args.port);
    let tcp_listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", addr);

    axum::serve(tcp_listener, app).await?;

    Ok(())
}
