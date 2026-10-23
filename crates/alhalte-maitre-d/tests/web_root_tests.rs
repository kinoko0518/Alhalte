//! SPA static-serving tests for `maitre-d --web-root`.

use std::sync::Arc;

use alhalte_maitre_d::{
    AppState, BanquetteEndpoint, BanquetteSpawner, Database, MaitreDError, MockOAuthProvider,
    RoomBridgeManager, create_router,
};
use tempfile::tempdir;

struct FailingSpawner;

#[async_trait::async_trait]
impl BanquetteSpawner for FailingSpawner {
    async fn spawn_banquette(
        &self,
        _room_id: u64,
        _admin_account: u64,
    ) -> alhalte_maitre_d::Result<BanquetteEndpoint> {
        Err(MaitreDError::Banquette("not used".to_string()))
    }
}

async fn serve(app: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

// spec: alhalte-maitre-d/web-root 0001 the SPA is served on the maitre-d origin
#[tokio::test]
async fn serves_the_spa_and_falls_back_for_client_routes() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<html>SPA</html>").unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/app.js"), "console.log('spa-asset')").unwrap();

    let manager = RoomBridgeManager::new(Arc::new(FailingSpawner));
    let state = AppState::new(
        Database::new_in_memory().unwrap(),
        "salt",
        Arc::new(MockOAuthProvider::new()),
        manager,
    )
    .with_web_root(dir.path());
    let addr = serve(create_router(state)).await;
    let client = reqwest::Client::new();

    let root = client.get(format!("http://{addr}/")).send().await.unwrap();
    assert_eq!(root.status(), reqwest::StatusCode::OK);
    assert!(root.text().await.unwrap().contains("SPA"));

    for path in ["/login?return_to=%2F", "/rooms/abcdef0123456789"] {
        let response = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "client route {path} must fall back to index.html"
        );
        assert!(response.text().await.unwrap().contains("SPA"));
    }

    let asset = client
        .get(format!("http://{addr}/assets/app.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status(), reqwest::StatusCode::OK);
    assert!(asset.text().await.unwrap().contains("spa-asset"));

    // Registered API routes still win over the SPA fallback.
    let api = client
        .get(format!("http://{addr}/api/session"))
        .send()
        .await
        .unwrap();
    assert_eq!(api.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert!(!api.text().await.unwrap().contains("SPA"));

    // The removed /api/server-info is a backend path, not a client route; the
    // SPA fallback must not turn it into index.html.
    let removed = client
        .get(format!("http://{addr}/api/server-info"))
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), reqwest::StatusCode::NOT_FOUND);
    assert!(!removed.text().await.unwrap().contains("SPA"));
}
