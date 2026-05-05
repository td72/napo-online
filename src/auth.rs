use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::{Request, State};
use axum::http::{HeaderName, header};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::AppState;
use crate::error::{AppError, AuthError};

const CF_HEADER: HeaderName = HeaderName::from_static("cf-access-jwt-assertion");
const CF_COOKIE: &str = "CF_Authorization=";

#[derive(Debug, Clone)]
pub struct Claims {
    pub sub: String,
    pub email: String,
}

impl Claims {
    fn dev(email: String) -> Self {
        Self {
            sub: email.clone(),
            email,
        }
    }
}

/// DEV_MODE 専用: クエリパラメータ `?dev_email=alice@local` から identity を取り出す。
/// 統合テスト/ローカル開発で複数プレイヤーを区別したいときに使う。
fn extract_dev_email(uri: &axum::http::Uri) -> Option<String> {
    let query = uri.query()?;
    for pair in query.split('&') {
        if let Some(rest) = pair.strip_prefix("dev_email=") {
            return Some(rest.to_string());
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct RawClaims {
    sub: String,
    email: String,
}

pub struct Verifier {
    team_domain: String,
    aud: String,
    jwks: RwLock<Option<JwkSet>>,
    http: reqwest::Client,
}

impl Verifier {
    pub fn new(team_domain: impl Into<String>, aud: impl Into<String>) -> Self {
        Self {
            team_domain: team_domain.into(),
            aud: aud.into(),
            jwks: RwLock::new(None),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn refresh(&self) -> Result<()> {
        let url = format!("https://{}/cdn-cgi/access/certs", self.team_domain);
        let jwks: JwkSet = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("fetch JWKS from {url}"))?
            .error_for_status()
            .context("JWKS HTTP status")?
            .json()
            .await
            .context("parse JWKS body")?;
        *self.jwks.write().await = Some(jwks);
        tracing::info!(team = %self.team_domain, "JWKS refreshed");
        Ok(())
    }

    pub async fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        let header =
            decode_header(token).map_err(|e| AuthError::InvalidToken(format!("header: {e}")))?;
        let kid = header.kid.ok_or(AuthError::MissingKid)?;

        let guard = self.jwks.read().await;
        let jwks = guard.as_ref().ok_or(AuthError::JwksNotLoaded)?;
        let jwk = jwks.find(&kid).ok_or(AuthError::UnknownKid)?;
        let key =
            DecodingKey::from_jwk(jwk).map_err(|e| AuthError::InvalidToken(format!("key: {e}")))?;

        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&self.aud]);
        validation.set_issuer(&[format!("https://{}", self.team_domain)]);

        let data = decode::<RawClaims>(token, &key, &validation)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;
        Ok(Claims {
            sub: data.claims.sub,
            email: data.claims.email,
        })
    }
}

pub fn spawn_jwks_refresher(verifier: Arc<Verifier>, every: Duration) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(every).await;
            if let Err(e) = verifier.refresh().await {
                tracing::warn!(error = %e, "JWKS refresh failed");
            }
        }
    });
}

pub async fn require_access(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let claims = if state.cfg.dev_mode {
        let email = extract_dev_email(req.uri()).unwrap_or_else(|| "dev@local".to_string());
        Claims::dev(email)
    } else {
        let verifier = state
            .verifier
            .as_ref()
            .expect("verifier must be set in non-dev mode");
        let token = extract_token(&req).ok_or(AuthError::MissingToken)?;
        verifier.verify(&token).await?
    };
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

fn extract_token(req: &Request) -> Option<String> {
    if let Some(v) = req.headers().get(&CF_HEADER)
        && let Ok(s) = v.to_str()
    {
        return Some(s.to_string());
    }
    let cookie = req.headers().get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        if let Some(rest) = part.trim().strip_prefix(CF_COOKIE) {
            return Some(rest.to_string());
        }
    }
    None
}
