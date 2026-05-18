use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::Claims;
use crate::error::AppError;

/// liveness: プロセスが生きてるだけを返す軽い check。LB や CF の health probe 用。
pub async fn healthz() -> StatusCode {
    StatusCode::OK
}

#[derive(Serialize)]
pub struct ReadyResponse {
    pub ok: bool,
    pub version: &'static str,
    pub checks: ReadyChecks,
}

#[derive(Serialize)]
pub struct ReadyChecks {
    pub db: bool,
    pub hub: bool,
}

/// readiness: 依存先まで叩いて成立性を確認する deep check。
/// deploy 後 smoke test と外形監視で使う想定。どれか false なら HTTP 503。
pub async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    // DB: 100ms timeout で SELECT 1
    let db_ok = tokio::time::timeout(std::time::Duration::from_millis(100), state.db.ping())
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
    // Hub: room_count() を読めれば OK(DashMap poison 検出)
    let hub_ok =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.hub.room_count())).is_ok();
    let ok = db_ok && hub_ok;
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(ReadyResponse {
            ok,
            version: env!("CARGO_PKG_VERSION"),
            checks: ReadyChecks {
                db: db_ok,
                hub: hub_ok,
            },
        }),
    )
}

#[derive(Serialize)]
pub struct MeResponse {
    pub email: String,
    /// 登録済み display_name。未登録なら None。
    pub handle: Option<String>,
    /// emoji 1 文字。未設定なら "" (空文字)。
    pub avatar: String,
    pub sub: String,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MeResponse>, AppError> {
    let user = state.db.get_user(&claims.email).await?;
    Ok(Json(MeResponse {
        email: claims.email.clone(),
        handle: user.as_ref().map(|u| u.handle.clone()),
        avatar: user.map(|u| u.avatar).unwrap_or_default(),
        sub: claims.sub,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateMeRequest {
    pub handle: String,
    pub avatar: String,
}

#[derive(Serialize)]
pub struct UpdateMeResponse {
    pub ok: bool,
    /// HandleTaken の場合は "handle_taken"、それ以外は None。
    pub error: Option<String>,
}

pub async fn update_me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<UpdateMeResponse>, AppError> {
    let handle = req.handle.trim();
    if handle.is_empty() {
        return Ok(Json(UpdateMeResponse {
            ok: false,
            error: Some("empty_handle".into()),
        }));
    }
    if handle.chars().count() > 32 {
        return Ok(Json(UpdateMeResponse {
            ok: false,
            error: Some("handle_too_long".into()),
        }));
    }
    let avatar = req.avatar.trim();
    if avatar.chars().count() > 4 {
        return Ok(Json(UpdateMeResponse {
            ok: false,
            error: Some("avatar_too_long".into()),
        }));
    }
    let res = state
        .db
        .set_user_profile(&claims.email, handle, avatar)
        .await?;
    Ok(Json(match res {
        crate::db::UpsertResult::Ok => UpdateMeResponse {
            ok: true,
            error: None,
        },
        crate::db::UpsertResult::HandleTaken => UpdateMeResponse {
            ok: false,
            error: Some("handle_taken".into()),
        },
    }))
}

#[derive(Serialize)]
pub struct UserSummary {
    pub email: String,
    pub handle: String,
    pub last_seen: i64,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct PagedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

fn normalize(p: &ListParams) -> (usize, usize) {
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = p.offset.unwrap_or(0);
    (limit, offset)
}

/// 過去に一度でも接続したユーザの一覧。CF Access の招待リスト本体ではないことに注意
/// (招待されたまま未ログインのメアドはここには出ない)。
pub async fn list_users(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<ListParams>,
) -> Result<Json<PagedResponse<UserSummary>>, AppError> {
    let (limit, offset) = normalize(&params);
    let total = state.db.count_users().await?;
    let users = state.db.list_users(limit, offset).await?;
    let items: Vec<UserSummary> = users
        .into_iter()
        .map(|u| UserSummary {
            email: u.email,
            handle: u.handle,
            last_seen: u.last_seen,
            created_at: u.created_at,
        })
        .collect();
    let has_more = offset + items.len() < total;
    Ok(Json(PagedResponse {
        items,
        total,
        limit,
        offset,
        has_more,
    }))
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

#[derive(Serialize)]
pub struct UserStatResponse {
    pub email: String,
    pub handle: String,
    pub played: i64,
    pub total_score: i64,
    pub as_napoleon: PerRoleStat,
    pub as_adjutant: PerRoleStat,
    pub as_ally: PerRoleStat,
}

#[derive(Serialize)]
pub struct PerRoleStat {
    pub count: i64,
    pub wins: i64,
}

/// プレイヤーごとの累積成績(終局済みの局のみ集計、total_score 降順)。
pub async fn list_stats(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<UserStatResponse>>, AppError> {
    let stats = state.db.list_user_stats().await?;
    let out = stats
        .into_iter()
        .map(|s| UserStatResponse {
            email: s.email,
            handle: s.handle,
            played: s.played,
            total_score: s.total_score,
            as_napoleon: PerRoleStat {
                count: s.as_napoleon_count,
                wins: s.as_napoleon_wins,
            },
            as_adjutant: PerRoleStat {
                count: s.as_adjutant_count,
                wins: s.as_adjutant_wins,
            },
            as_ally: PerRoleStat {
                count: s.as_ally_count,
                wins: s.as_ally_wins,
            },
        })
        .collect();
    Ok(Json(out))
}

#[derive(Serialize)]
pub struct RoomListResponse {
    pub rooms: Vec<RoomSummaryResponse>,
}

#[derive(Serialize)]
pub struct RoomSummaryResponse {
    pub room_id: String,
    pub host: String,
    pub phase: String,
    pub max_players: usize,
    pub member_count: usize,
    pub joinable: bool,
    pub members: Vec<RoomMemberResponse>,
    pub updated_at: i64,
}

#[derive(Serialize)]
pub struct RoomMemberResponse {
    pub display_name: String,
    pub is_com: bool,
}

/// 現存している全 room の軽量サマリ。Lobby は誰でも join 可、
/// それ以外は既存メンバーのみ復帰可。
pub async fn list_rooms(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<RoomListResponse>, AppError> {
    let summaries = state.db.list_room_summaries().await?;
    let rooms = summaries
        .into_iter()
        .map(|s| {
            let joinable = s.phase == "Lobby" && s.members.len() < s.max_players;
            RoomSummaryResponse {
                room_id: s.room_id,
                host: s.host,
                phase: s.phase,
                max_players: s.max_players,
                member_count: s.members.len(),
                joinable,
                members: s
                    .members
                    .into_iter()
                    .map(|m| RoomMemberResponse {
                        display_name: m.display_name,
                        is_com: m.is_com,
                    })
                    .collect(),
                updated_at: s.updated_at,
            }
        })
        .collect();
    Ok(Json(RoomListResponse { rooms }))
}

/// 終局済みの局を新しい順に返す。`?limit=N&offset=M`(デフォルト 20、最大 100)。
pub async fn list_games(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<ListParams>,
) -> Result<Json<PagedResponse<GameSummaryResponse>>, AppError> {
    let (limit, offset) = normalize(&params);
    let total = state.db.count_finished_games().await?;
    let games = state.db.list_recent_games(limit, offset).await?;
    let items: Vec<GameSummaryResponse> = games
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
    let has_more = offset + items.len() < total;
    Ok(Json(PagedResponse {
        items,
        total,
        limit,
        offset,
        has_more,
    }))
}
