use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Serialize;

use crate::AppState;
use crate::auth::Claims;
use crate::error::AppError;

pub async fn healthz() -> StatusCode {
    StatusCode::OK
}

#[derive(Serialize)]
pub struct MeResponse {
    pub email: String,
    /// 登録済み display_name。未登録なら None。
    pub handle: Option<String>,
    pub sub: String,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MeResponse>, AppError> {
    let user = state.db.get_user(&claims.email).await?;
    Ok(Json(MeResponse {
        email: claims.email.clone(),
        handle: user.map(|u| u.handle),
        sub: claims.sub,
    }))
}
