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
    let local_login = args.local_login;
    // `--local-login` switches to the isolated local SQLite file; the normal
    // Google database is never opened while it is active.
    let db_path = args.active_db_path();
    tracing::info!(
        "Starting alhalte-maitre-d on {}:{} (db: {:?})",
        args.host,
        args.port,
        db_path
    );
    if local_login {
        tracing::warn!(
            "Local password login is ENABLED: accounts, sessions, and rooms use the isolated \
             database {:?}; room state is still persisted to cellar. The login is HTTP-only, so \
             use this only on a trusted LAN",
            db_path
        );
    }

    let db = Database::open(&db_path)?;
    tracing::info!("SQLite database opened at {:?}", db_path);

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
    )
    .with_heartbeat_interval_secs(args.banquette_heartbeat_interval_secs);
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

    let room_manager = RoomBridgeManager::new(spawner);

    let mut state = AppState::new(db, args.salt, oauth, room_manager)
        .with_public_url(&args.public_url)
        .with_cellar_proxy(CellarProxy::new(&args.cellar_url)?);
    if local_login {
        state = state.with_local_login();
    } else {
        tracing::info!("Google OAuth redirect URI: {}", state.oauth_redirect_uri());
    }
    if let Some(web_root) = &args.web_root {
        tracing::info!("Serving SPA from {:?}", web_root);
        state = state.with_web_root(web_root);
    }
    if !args.cors_origins.is_empty() {
        tracing::info!("CORS allowed origins: {}", args.cors_origins.join(", "));
        state = state.with_allowed_origins(args.cors_origins);
    }
    // Issue #229: development-only, opt-in private-network origins. alhalte-restaurant
    // emits the port only for the bundled local-development configuration.
    if !args.allow_lan_origin_ports.is_empty() {
        tracing::warn!(
            "Private-network LAN origins enabled for http://<private IPv4>:{:?}; never use this on a public network",
            args.allow_lan_origin_ports
        );
        state = state.with_lan_origin_ports(args.allow_lan_origin_ports);
    }
    let app = create_router(state);

    // If an optional UNIX domain socket is specified, also serve on it.
    #[cfg(unix)]
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
    #[cfg(not(unix))]
    if args.unix_socket.is_some() {
        return Err(std::io::Error::other(
            "--unix-socket is not supported on Windows; use the TCP listener",
        )
        .into());
    }

    let addr = format!("{}:{}", args.host, args.port);
    let tcp_listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", addr);

    axum::serve(tcp_listener, app).await?;

    Ok(())
}
