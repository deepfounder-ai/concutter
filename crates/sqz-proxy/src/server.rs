use std::sync::Arc;

use crate::state::AppState;

/// Start the HTTP server and listen on the given host:port.
///
/// The server will run until it receives a Ctrl-C signal, at which point it
/// performs a graceful shutdown (finishing in-flight requests before exiting).
pub async fn run_server(state: Arc<AppState>, host: &str, port: u16) -> anyhow::Result<()> {
    let app = crate::router::build_router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("sqz proxy listening on {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Wait for a Ctrl-C signal.
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutdown signal received");
}
