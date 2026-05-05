mod config;
mod http;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    init_tracing(&cfg.log);

    tracing::info!(
        bind = %cfg.bind,
        dev_mode = cfg.dev_mode,
        "starting napo-online"
    );

    let app = Router::new().route("/healthz", get(http::healthz));

    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing(filter: &str) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .json()
        .init();
}
