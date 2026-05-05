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

#[derive(Serialize)]
pub struct UserSummary {
    pub email: String,
    pub handle: String,
    pub last_seen: i64,
    pub created_at: i64,
}

/// 過去に一度でも接続したユーザの一覧。CF Access の招待リスト本体ではないことに注意
/// (招待されたまま未ログインのメアドはここには出ない)。
pub async fn list_users(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<UserSummary>>, AppError> {
    let users = state.db.list_users().await?;
    let summaries = users
        .into_iter()
        .map(|u| UserSummary {
            email: u.email,
            handle: u.handle,
            last_seen: u.last_seen,
            created_at: u.created_at,
        })
        .collect();
    Ok(Json(summaries))
}
