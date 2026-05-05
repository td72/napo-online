mod auth;
mod config;
mod error;
mod http;
mod protocol;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use axum::middleware;
use tracing_subscriber::EnvFilter;

use crate::auth::Verifier;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub verifier: Option<Arc<Verifier>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Arc::new(Config::from_env()?);
    init_tracing(&cfg.log);

    let verifier = build_verifier(&cfg).await?;
    let state = AppState {
        cfg: cfg.clone(),
        verifier,
    };

    tracing::info!(
        bind = %cfg.bind,
        dev_mode = cfg.dev_mode,
        "starting napo-online"
    );

    let protected = Router::new()
        .route("/api/me", get(http::me))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_access,
        ));

    let app = Router::new()
        .route("/healthz", get(http::healthz))
        .merge(protected)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn build_verifier(cfg: &Config) -> Result<Option<Arc<Verifier>>> {
    if cfg.dev_mode {
        tracing::warn!("DEV_MODE active — Cloudflare Access JWT verification is bypassed");
        return Ok(None);
    }
    let team = cfg.cf_team.as_ref().expect("cf_team checked in Config");
    let aud = cfg.cf_aud.as_ref().expect("cf_aud checked in Config");
    let v = Arc::new(Verifier::new(team.clone(), aud.clone()));
    if let Err(e) = v.refresh().await {
        tracing::warn!(error = %e, "initial JWKS refresh failed; will retry in background");
    }
    auth::spawn_jwks_refresher(v.clone(), Duration::from_secs(3600));
    Ok(Some(v))
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
