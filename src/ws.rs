use axum::Extension;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

use crate::AppState;
use crate::auth::Claims;
use crate::protocol::{ClientMessage, ServerMessage};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> impl IntoResponse {
    let max = state.cfg.ws_max_msg_bytes;
    ws.max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| handle_socket(socket, claims))
}

async fn handle_socket(mut socket: WebSocket, claims: Claims) {
    tracing::info!(email = %claims.email, "ws connected");

    let welcome = ServerMessage::Welcome {
        player_id: claims.email.clone(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    if send(&mut socket, &welcome).await.is_err() {
        return;
    }

    while let Some(frame) = socket.recv().await {
        let frame = match frame {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error = %e, "ws recv error");
                break;
            }
        };
        match frame {
            Message::Text(text) => {
                if !handle_text(&mut socket, &claims, text.as_ref()).await {
                    break;
                }
            }
            Message::Binary(_) => {
                let _ = send_error(
                    &mut socket,
                    "binary_not_supported",
                    "binary frames are not allowed",
                )
                .await;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {
                // axum/tungstenite が WS ping/pong は自動応答する。
                // アプリ層の Ping は ClientMessage::Ping(JSON)で扱う。
            }
        }
    }
    tracing::info!(email = %claims.email, "ws closed");
}

/// returns false to indicate the connection should close
async fn handle_text(socket: &mut WebSocket, claims: &Claims, text: &str) -> bool {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            let _ = send_error(socket, "invalid_message", &e.to_string()).await;
            return true;
        }
    };
    match msg {
        ClientMessage::Hello { display_name } => {
            tracing::info!(email = %claims.email, %display_name, "hello");
            // Welcome は接続時に送信済み。display_name は後で Hub の PlayerInfo に保存する。
        }
        ClientMessage::Ping => {
            return send(socket, &ServerMessage::Pong).await.is_ok();
        }
        ClientMessage::CreateRoom | ClientMessage::JoinRoom { .. } | ClientMessage::LeaveRoom => {
            let _ = send_error(socket, "not_implemented", "Hub not yet implemented").await;
        }
    }
    true
}

async fn send(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), axum::Error> {
    let s = serde_json::to_string(msg).expect("serialize ServerMessage");
    socket.send(Message::Text(s.into())).await
}

async fn send_error(socket: &mut WebSocket, code: &str, message: &str) -> Result<(), axum::Error> {
    send(
        socket,
        &ServerMessage::Error {
            code: code.into(),
            message: message.into(),
        },
    )
    .await
}
