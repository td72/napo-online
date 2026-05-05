use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Serialize;

use crate::auth::Claims;

pub async fn healthz() -> StatusCode {
    StatusCode::OK
}

#[derive(Serialize)]
pub struct MeResponse {
    pub email: String,
    pub sub: String,
}

pub async fn me(Extension(claims): Extension<Claims>) -> Json<MeResponse> {
    Json(MeResponse {
        email: claims.email,
        sub: claims.sub,
    })
}
