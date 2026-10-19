//! CLI binary for `alhalte-cellar`.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use alhalte_cellar::verifier::VerifierMode;
use alhalte_cellar::{CellarConfig, CellarService, CliArgs};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = CliArgs::parse();

    let allow_all_sessions = args.allow_all_sessions;
    let endpoint_configured = args.maitre_d_url.is_some() || args.maitre_d_socket.is_some();
    let verifier_mode =
        VerifierMode::resolve(args.maitre_d_url, args.maitre_d_socket, allow_all_sessions)?;

    if verifier_mode.is_allow_all() {
        tracing::warn!("Session verifier mode: {verifier_mode} (every session will be accepted)");
        if endpoint_configured {
            tracing::warn!(
                "--allow-all-sessions takes precedence over the configured maitre-d endpoint"
            );
        }
    } else {
        tracing::info!("Session verifier mode: {verifier_mode}");
    }
    let verifier = verifier_mode.into_verifier();

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

    let mut service = CellarService::new(&config, verifier)?;
    match args.service_token.as_deref() {
        Some(token) => {
            tracing::info!("Internal service token configured for trusted room-state persistence");
            service = service.with_service_token(token);
        }
        None => tracing::warn!(
            "No --service-token configured: internal room-state persistence from banquette \
             will be rejected unless a user session is supplied"
        ),
    }
    let service = Arc::new(service);
    let _gc_handle = service.spawn_gc_task();

    let app = service.into_router();

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    tracing::info!("alhalte-cellar listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
