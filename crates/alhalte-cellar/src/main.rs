//! CLI binary for `alhalte-cellar`.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alhalte_cellar::verifier::{InMemorySessionVerifier, MaitreDVerifier, SessionVerifier};
use alhalte_cellar::{CellarConfig, CellarService, CliArgs};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = CliArgs::parse();

    let verifier: Arc<dyn SessionVerifier> = if args.allow_all_sessions {
        tracing::warn!("Using InMemorySessionVerifier with allow_all = true (development mode)");
        Arc::new(InMemorySessionVerifier::allow_all_verifier())
    } else if let Some(socket_path) = args.maitre_d_socket {
        tracing::info!(
            "Using MaitreDVerifier via unix socket: {}",
            socket_path.display()
        );
        Arc::new(MaitreDVerifier::new_unix(socket_path))
    } else if let Some(url) = args.maitre_d_url {
        tracing::info!("Using MaitreDVerifier via HTTP: {}", url);
        Arc::new(MaitreDVerifier::new_http(url))
    } else {
        tracing::info!("No maitre-d endpoint specified, defaulting to InMemorySessionVerifier");
        Arc::new(InMemorySessionVerifier::new())
    };

    let config = CellarConfig {
        host: args.host.clone(),
        port: args.port,
        storage_path: args.storage_path,
        db_path: args.db_path,
        anti_oracle: args.anti_oracle,
        gc_interval: if args.gc_interval_secs > 0 {
            Some(Duration::from_secs(args.gc_interval_secs))
        } else {
            None
        },
    };

    let service = Arc::new(CellarService::new(&config, verifier)?);
    let _gc_handle = service.spawn_gc_task();

    let app = service.into_router();

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    tracing::info!("alhalte-cellar listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
