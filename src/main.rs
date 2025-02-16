use std::env;

use axum::{
    routing::{get, post},
    Router,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod handlers;
mod proto;

fn init_registry() {
    let registry = tracing_subscriber::registry().with(
        tracing_subscriber::filter::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "debug,hyper_util=info".into()),
    );
    if cfg!(debug_assertions) {
        registry.with(tracing_subscriber::fmt::layer()).init();
    } else {
        registry
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    }
}

#[tokio::main]
async fn main() {
    init_registry();

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        // upload
        .route("/v1/upload/limit", get(handlers::upload::upload_get_limit))
        .route("/v1/upload/start", post(handlers::upload::upload_start))
        .route("/v1/upload/chunk", post(handlers::upload::upload_chunk))
        .route("/v1/upload/finalize", post(handlers::upload::upload_finalize))
        // files
        .route("/v1/files/{ref}/chunks/{offset}", get(handlers::files::file_chunk))
        .route("/v1/files/{ref}/meta", get(handlers::files::file_meta))
    ;

    let addr = env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.unwrap();
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}