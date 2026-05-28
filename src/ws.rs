use std::time::Instant;

use axum::Extension;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::mpsc;

use crate::AppState;
use crate::auth::Claims;
use crate::db::UpsertResult;
use crate::hub::{CreateRoomError, JoinError, RoomHandle};
use crate::now_unix;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::room::{PlayerInfo, RoomCommand};

/// 1接続あたりのレート制限。`refill_per_sec` トークン/秒の充填、上限 `capacity`(burst)。
/// `try_take` で 1メッセージ消費を試行。
#[derive(Debug)]
struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    fn new(refill_per_sec: u32, burst: u32) -> Self {
        let capacity = burst as f64;
        Self {
            capacity,
            refill_per_sec: refill_per_sec as f64,
            tokens: capacity,
            last: Instant::now(),
        }
    }

    fn try_take(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

const OUTBOUND_BUFFER: usize = 64;
const DEFAULT_DISPLAY_NAME: &str = "anonymous";

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> impl IntoResponse {
    let max = state.cfg.ws_max_msg_bytes;
    ws.max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| handle_socket(socket, claims, state))
}

struct Session {
    claims: Claims,
    state: AppState,
    msg_tx: mpsc::Sender<ServerMessage>,
    display_name: String,
    avatar: String,
    current_room: Option<RoomHandle>,
    bucket: TokenBucket,
    /// 連続でレート制限に引っかかった回数。閾値超過で切断
    consecutive_throttles: u32,
}

const MAX_CONSECUTIVE_THROTTLES: u32 = 30;

fn client_msg_type(msg: &ClientMessage) -> &'static str {
    match msg {
        ClientMessage::Hello { .. } => "hello",
        ClientMessage::CreateRoom { .. } => "create_room",
        ClientMessage::JoinRoom { .. } => "join_room",
        ClientMessage::LeaveRoom => "leave_room",
        ClientMessage::StartMatch => "start_match",
        ClientMessage::AbortHand => "abort_hand",
        ClientMessage::EndMatch => "end_match",
        ClientMessage::AddCom { .. } => "add_com",
        ClientMessage::ManualBid { .. } => "manual_bid",
        ClientMessage::NextHand => "next_hand",
        ClientMessage::SubmitAction { .. } => "submit_action",
        ClientMessage::Chat { .. } => "chat",
        ClientMessage::Ping => "ping",
    }
}

fn server_msg_type(msg: &ServerMessage) -> &'static str {
    match msg {
        ServerMessage::Welcome { .. } => "welcome",
        ServerMessage::RoomState { .. } => "room_state",
        ServerMessage::Error { .. } => "error",
        ServerMessage::Pong => "pong",
        ServerMessage::Event { .. } => "event",
        ServerMessage::Observation { .. } => "observation",
        ServerMessage::MatchStarted { .. } => "match_started",
        ServerMessage::HandEnded { .. } => "hand_ended",
        ServerMessage::NextHandReady { .. } => "next_hand_ready",
        ServerMessage::MatchEnded { .. } => "match_ended",
        ServerMessage::PlayerDisconnected { .. } => "player_disconnected",
        ServerMessage::PlayerReconnected { .. } => "player_reconnected",
        ServerMessage::Chat { .. } => "chat",
        ServerMessage::ChatBacklog { .. } => "chat_backlog",
    }
}

async fn handle_socket(mut socket: WebSocket, claims: Claims, state: AppState) {
    metrics::gauge!("napo_players_online").increment(1.0);
    tracing::info!(email = %claims.email, "ws connected");

    // touch last_seen と既存ハンドル取得(再接続でも display_name を保持)
    let _ = state.db.touch_last_seen(&claims.email, now_unix()).await;
    let existing_user = state.db.get_user(&claims.email).await.ok().flatten();
    let existing_handle = existing_user.as_ref().map(|u| u.handle.clone());
    let existing_avatar = existing_user
        .as_ref()
        .map(|u| u.avatar.clone())
        .unwrap_or_default();

    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(OUTBOUND_BUFFER);

    // 切断 → 再接続時に自動で部屋に復帰できるよう、snapshot に member として残っている
    // 部屋を探して welcome に同梱する。
    let current_room = state
        .db
        .find_room_for_member(&claims.email)
        .await
        .ok()
        .flatten();
    let welcome = ServerMessage::Welcome {
        player_id: claims.email.clone(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        current_room,
    };
    if send(&mut socket, &welcome).await.is_err() {
        return;
    }

    let t1 = session_state_t1(&state);
    let rate = state.cfg.ws_rate_limit_per_sec;
    let mut session = Session {
        claims,
        state,
        msg_tx,
        display_name: existing_handle.unwrap_or_else(|| DEFAULT_DISPLAY_NAME.to_string()),
        avatar: existing_avatar,
        current_room: None,
        bucket: TokenBucket::new(rate, rate.saturating_mul(2).max(1)),
        consecutive_throttles: 0,
    };
    let mut last_activity = Instant::now();

    loop {
        let now = Instant::now();
        let deadline = last_activity + t1;
        let until = deadline.saturating_duration_since(now);

        tokio::select! {
            biased;
            Some(out) = msg_rx.recv() => {
                if send(&mut socket, &out).await.is_err() { break; }
            }
            frame = socket.recv() => {
                let Some(Ok(frame)) = frame else { break };
                last_activity = Instant::now();
                let keep = match frame {
                    Message::Text(text) => handle_text(&mut socket, &mut session, text.as_ref()).await,
                    Message::Binary(_) => {
                        send(
                            &mut socket,
                            &error_msg("binary_not_supported", "binary frames are not allowed"),
                        )
                        .await
                        .is_ok()
                    }
                    Message::Close(_) => false,
                    Message::Ping(_) | Message::Pong(_) => true,
                };
                if !keep { break; }
            }
            _ = tokio::time::sleep(until) => {
                tracing::warn!(
                    email = %session.claims.email,
                    idle_secs = (Instant::now() - last_activity).as_secs(),
                    "ws idle timeout (T1) — closing"
                );
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
        }
    }

    if let Some(rh) = session.current_room.take() {
        let _ = rh
            .tx
            .send(RoomCommand::Disconnect {
                player: session.claims.email.clone(),
            })
            .await;
    }
    metrics::gauge!("napo_players_online").decrement(1.0);
    tracing::info!(email = %session.claims.email, "ws closed");
}

async fn handle_text(socket: &mut WebSocket, session: &mut Session, text: &str) -> bool {
    if !session.bucket.try_take(Instant::now()) {
        session.consecutive_throttles += 1;
        let _ = send(
            socket,
            &error_msg(
                "rate_limited",
                "too many messages — slow down or you'll be disconnected",
            ),
        )
        .await;
        if session.consecutive_throttles >= MAX_CONSECUTIVE_THROTTLES {
            tracing::warn!(
                email = %session.claims.email,
                consecutive = session.consecutive_throttles,
                "ws rate limit exceeded — closing"
            );
            let _ = socket.send(Message::Close(None)).await;
            return false;
        }
        return true;
    }
    session.consecutive_throttles = 0;

    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            return send(socket, &error_msg("invalid_message", &e.to_string()))
                .await
                .is_ok();
        }
    };
    metrics::counter!("napo_ws_messages_total", "direction" => "in", "type" => client_msg_type(&msg)).increment(1);
    match msg {
        ClientMessage::Hello { display_name } => {
            tracing::info!(email = %session.claims.email, %display_name, "hello");
            if display_name.trim().is_empty() {
                let _ = send(
                    socket,
                    &error_msg("invalid_handle", "display_name must not be empty"),
                )
                .await;
                return true;
            }
            match session
                .state
                .db
                .upsert_user(&session.claims.email, &display_name, now_unix())
                .await
            {
                Ok(UpsertResult::Ok) => {
                    session.display_name = display_name;
                }
                Ok(UpsertResult::HandleTaken) => {
                    let _ = send(
                        socket,
                        &error_msg(
                            "handle_taken",
                            &format!("handle '{display_name}' is already taken"),
                        ),
                    )
                    .await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "upsert_user failed");
                    let _ = send(socket, &error_msg("internal", "failed to update profile")).await;
                }
            }
            true
        }
        ClientMessage::Ping => send(socket, &ServerMessage::Pong).await.is_ok(),
        ClientMessage::CreateRoom { bidding_mode } => {
            handle_create_room(socket, session, bidding_mode).await;
            true
        }
        ClientMessage::JoinRoom { room_id } => {
            handle_join_room(socket, session, &room_id).await;
            true
        }
        ClientMessage::LeaveRoom => {
            handle_leave_room(session).await;
            true
        }
        ClientMessage::StartMatch => {
            forward_to_room(
                socket,
                session,
                RoomCommand::StartMatch {
                    by: session.claims.email.clone(),
                },
            )
            .await
        }
        ClientMessage::AbortHand => {
            forward_to_room(
                socket,
                session,
                RoomCommand::AbortHand {
                    by: session.claims.email.clone(),
                },
            )
            .await
        }
        ClientMessage::EndMatch => {
            forward_to_room(
                socket,
                session,
                RoomCommand::EndMatch {
                    by: session.claims.email.clone(),
                },
            )
            .await
        }
        ClientMessage::NextHand => {
            forward_to_room(
                socket,
                session,
                RoomCommand::NextHand {
                    player: session.claims.email.clone(),
                },
            )
            .await
        }
        ClientMessage::SubmitAction { action } => {
            forward_to_room(
                socket,
                session,
                RoomCommand::SubmitAction {
                    player: session.claims.email.clone(),
                    action,
                },
            )
            .await
        }
        ClientMessage::AddCom { display_name } => {
            forward_to_room(
                socket,
                session,
                RoomCommand::AddCom {
                    by: session.claims.email.clone(),
                    display_name,
                },
            )
            .await
        }
        ClientMessage::ManualBid { declarer_seat, bid } => {
            forward_to_room(
                socket,
                session,
                RoomCommand::ManualBid {
                    by: session.claims.email.clone(),
                    declarer_seat,
                    bid,
                },
            )
            .await
        }
        ClientMessage::Chat { text } => {
            forward_to_room(
                socket,
                session,
                RoomCommand::Chat {
                    player: session.claims.email.clone(),
                    text,
                },
            )
            .await
        }
    }
}

async fn forward_to_room(socket: &mut WebSocket, session: &Session, cmd: RoomCommand) -> bool {
    let Some(rh) = session.current_room.as_ref() else {
        let _ = send(socket, &error_msg("no_room", "you are not in a room")).await;
        return true;
    };
    if rh.tx.send(cmd).await.is_err() {
        let _ = send(
            socket,
            &error_msg("room_closed", "room is no longer reachable"),
        )
        .await;
    }
    true
}

async fn handle_create_room(
    socket: &mut WebSocket,
    session: &mut Session,
    bidding_mode: crate::protocol::BiddingMode,
) {
    match session
        .state
        .hub
        .create_room(session.claims.email.clone(), bidding_mode)
    {
        Ok(room_id) => {
            join_room_inner(socket, session, &room_id).await;
        }
        Err(CreateRoomError::MaxRoomsReached) => {
            let _ = send(
                socket,
                &error_msg("max_rooms_reached", "server is at room capacity"),
            )
            .await;
        }
    }
}

async fn handle_join_room(socket: &mut WebSocket, session: &mut Session, room_id: &str) {
    join_room_inner(socket, session, room_id).await;
}

async fn join_room_inner(socket: &mut WebSocket, session: &mut Session, room_id: &str) {
    // 既存の部屋から抜ける
    if let Some(rh) = session.current_room.take() {
        let _ = rh
            .tx
            .send(RoomCommand::Leave {
                player: session.claims.email.clone(),
            })
            .await;
    }
    let info = PlayerInfo {
        player_id: session.claims.email.clone(),
        display_name: session.display_name.clone(),
        avatar: session.avatar.clone(),
    };
    let owned_id = room_id.to_string();
    match session
        .state
        .hub
        .join(&owned_id, info, session.msg_tx.clone())
        .await
    {
        Ok(rh) => {
            session.current_room = Some(rh);
        }
        Err(JoinError::NotFound) => {
            let _ = send(
                socket,
                &error_msg("room_not_found", &format!("room {owned_id} not found")),
            )
            .await;
        }
        Err(JoinError::Closed) => {
            let _ = send(
                socket,
                &error_msg("room_closed", "room is no longer accepting joins"),
            )
            .await;
        }
    }
}

async fn handle_leave_room(session: &mut Session) {
    if let Some(rh) = session.current_room.take() {
        let _ = rh
            .tx
            .send(RoomCommand::Leave {
                player: session.claims.email.clone(),
            })
            .await;
    }
}

async fn send(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), axum::Error> {
    metrics::counter!("napo_ws_messages_total", "direction" => "out", "type" => server_msg_type(msg)).increment(1);
    let s = serde_json::to_string(msg).expect("serialize ServerMessage");
    socket.send(Message::Text(s.into())).await
}

fn error_msg(code: &str, message: &str) -> ServerMessage {
    ServerMessage::Error {
        code: code.into(),
        message: message.into(),
    }
}

fn session_state_t1(state: &AppState) -> std::time::Duration {
    state.cfg.t1_detect
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_bucket_starts_full_and_refills() {
        let start = Instant::now();
        let mut b = TokenBucket {
            capacity: 5.0,
            refill_per_sec: 10.0,
            tokens: 5.0,
            last: start,
        };
        // 5発までは即時取れる(burst 5)
        for _ in 0..5 {
            assert!(b.try_take(start));
        }
        // 直後の 6発目はダメ
        assert!(!b.try_take(start));
        // 0.3 秒後 = 3 トークン補充されているはず → 3発取れる
        let later = start + Duration::from_millis(300);
        assert!(b.try_take(later));
        assert!(b.try_take(later));
        assert!(b.try_take(later));
        assert!(!b.try_take(later));
    }

    #[test]
    fn token_bucket_caps_at_capacity_after_long_idle() {
        let start = Instant::now();
        let mut b = TokenBucket {
            capacity: 5.0,
            refill_per_sec: 10.0,
            tokens: 0.0,
            last: start,
        };
        // 1 時間アイドル後でも capacity 上限まで
        let later = start + Duration::from_secs(3600);
        for _ in 0..5 {
            assert!(b.try_take(later));
        }
        assert!(!b.try_take(later));
    }
}
