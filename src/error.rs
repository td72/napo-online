use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing CF Access JWT")]
    MissingToken,
    #[error("invalid token: {0}")]
    InvalidToken(String),
    #[error("JWKS not loaded")]
    JwksNotLoaded,
    #[error("unknown key id")]
    UnknownKid,
    #[error("token header missing kid")]
    MissingKid,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::Auth(AuthError::MissingToken) => (StatusCode::UNAUTHORIZED, "missing_token"),
            AppError::Auth(AuthError::InvalidToken(_)) => {
                (StatusCode::UNAUTHORIZED, "invalid_token")
            }
            AppError::Auth(AuthError::MissingKid) => (StatusCode::UNAUTHORIZED, "missing_kid"),
            AppError::Auth(AuthError::UnknownKid) => (StatusCode::UNAUTHORIZED, "unknown_kid"),
            AppError::Auth(AuthError::JwksNotLoaded) => {
                (StatusCode::SERVICE_UNAVAILABLE, "jwks_not_loaded")
            }
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        tracing::warn!(error = %self, code = code, "request error");
        let body = json!({ "code": code, "message": self.to_string() });
        (status, Json(body)).into_response()
    }
}
