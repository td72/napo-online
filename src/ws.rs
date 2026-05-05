use axum::Extension;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::mpsc;

use crate::AppState;
use crate::auth::Claims;
use crate::hub::{CreateRoomError, JoinError, RoomHandle};
use crate::now_unix;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::room::{PlayerInfo, RoomCommand};

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
    current_room: Option<RoomHandle>,
}

async fn handle_socket(mut socket: WebSocket, claims: Claims, state: AppState) {
    tracing::info!(email = %claims.email, "ws connected");

    // touch last_seen と既存ハンドル取得(再接続でも display_name を保持)
    let _ = state.db.touch_last_seen(&claims.email, now_unix()).await;
    let existing_handle = state
        .db
        .get_user(&claims.email)
        .await
        .ok()
        .flatten()
        .map(|u| u.handle);

    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(OUTBOUND_BUFFER);

    let welcome = ServerMessage::Welcome {
        player_id: claims.email.clone(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    if send(&mut socket, &welcome).await.is_err() {
        return;
    }

    let mut session = Session {
        claims,
        state,
        msg_tx,
        display_name: existing_handle.unwrap_or_else(|| DEFAULT_DISPLAY_NAME.to_string()),
        current_room: None,
    };

    loop {
        tokio::select! {
            biased;
            Some(out) = msg_rx.recv() => {
                if send(&mut socket, &out).await.is_err() { break; }
            }
            frame = socket.recv() => {
                let Some(Ok(frame)) = frame else { break };
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
    tracing::info!(email = %session.claims.email, "ws closed");
}

async fn handle_text(socket: &mut WebSocket, session: &mut Session, text: &str) -> bool {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            return send(socket, &error_msg("invalid_message", &e.to_string()))
                .await
                .is_ok();
        }
    };
    match msg {
        ClientMessage::Hello { display_name } => {
            tracing::info!(email = %session.claims.email, %display_name, "hello");
            if let Err(e) = session
                .state
                .db
                .upsert_user(&session.claims.email, &display_name, now_unix())
                .await
            {
                tracing::warn!(error = %e, "upsert_user failed");
            }
            session.display_name = display_name;
            true
        }
        ClientMessage::Ping => send(socket, &ServerMessage::Pong).await.is_ok(),
        ClientMessage::CreateRoom => {
            handle_create_room(socket, session).await;
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
            forward_to_room(socket, session, RoomCommand::StartMatch {
                by: session.claims.email.clone(),
            })
            .await
        }
        ClientMessage::AbortHand => {
            forward_to_room(socket, session, RoomCommand::AbortHand {
                by: session.claims.email.clone(),
            })
            .await
        }
        ClientMessage::EndMatch => {
            forward_to_room(socket, session, RoomCommand::EndMatch {
                by: session.claims.email.clone(),
            })
            .await
        }
        ClientMessage::NextHand => {
            forward_to_room(socket, session, RoomCommand::NextHand {
                player: session.claims.email.clone(),
            })
            .await
        }
        ClientMessage::SubmitAction { action } => {
            forward_to_room(socket, session, RoomCommand::SubmitAction {
                player: session.claims.email.clone(),
                action,
            })
            .await
        }
    }
}

async fn forward_to_room(
    socket: &mut WebSocket,
    session: &Session,
    cmd: RoomCommand,
) -> bool {
    let Some(rh) = session.current_room.as_ref() else {
        let _ = send(socket, &error_msg("no_room", "you are not in a room")).await;
        return true;
    };
    if rh.tx.send(cmd).await.is_err() {
        let _ = send(socket, &error_msg("room_closed", "room is no longer reachable")).await;
    }
    true
}

async fn handle_create_room(socket: &mut WebSocket, session: &mut Session) {
    match session
        .state
        .hub
        .create_room(session.claims.email.clone())
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
    join_room_inner(socket, session, &room_id.to_string()).await;
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
    let s = serde_json::to_string(msg).expect("serialize ServerMessage");
    socket.send(Message::Text(s.into())).await
}

fn error_msg(code: &str, message: &str) -> ServerMessage {
    ServerMessage::Error {
        code: code.into(),
        message: message.into(),
    }
}
