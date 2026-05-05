use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize)]
pub struct ListGamesParams {
    /// 取得件数。デフォルト 20、最大 100。
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct GameSummaryResponse {
    pub id: String,
    pub room_id: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub napoleon: Option<PlayerRef>,
    pub adjutant: Option<PlayerRef>,
    pub bid_value: Option<i32>,
    pub bid_suit: Option<String>,
    pub napoleon_won: Option<bool>,
    pub players: Vec<PlayerScore>,
}

#[derive(Serialize)]
pub struct PlayerRef {
    pub email: String,
    pub handle: Option<String>,
}

#[derive(Serialize)]
pub struct PlayerScore {
    pub seat: usize,
    pub email: String,
    pub handle: Option<String>,
    pub score: Option<i32>,
}

/// 終局済みの局を新しい順に返す。`?limit=N`(デフォルト 20、最大 100)。
pub async fn list_games(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<ListGamesParams>,
) -> Result<Json<Vec<GameSummaryResponse>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let games = state.db.list_recent_games(limit).await?;
    let out = games
        .into_iter()
        .map(|g| GameSummaryResponse {
            id: g.id,
            room_id: g.room_id,
            started_at: g.started_at,
            ended_at: g.ended_at.unwrap_or(0),
            napoleon: g.napoleon_email.map(|email| PlayerRef {
                email,
                handle: g.napoleon_handle,
            }),
            adjutant: g.adjutant_email.map(|email| PlayerRef {
                email,
                handle: g.adjutant_handle,
            }),
            bid_value: g.bid_value,
            bid_suit: g.bid_suit,
            napoleon_won: g.napoleon_won,
            players: g
                .players
                .into_iter()
                .map(|p| PlayerScore {
                    seat: p.seat,
                    email: p.email,
                    handle: p.handle,
                    score: p.score,
                })
                .collect(),
        })
        .collect();
    Ok(Json(out))
}
