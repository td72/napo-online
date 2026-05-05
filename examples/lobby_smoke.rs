//! ロビー手動 smoke テスト。
//!
//! 1) napo-online を `NAPO_DEV_MODE=true cargo run` で起動
//! 2) このサンプルを `cargo run --example lobby_smoke` で実行
//!
//! 2接続を立てて以下を確認:
//!   - A が CreateRoom → Welcome + RoomState を受信
//!   - B が JoinRoom → 自分の RoomState、A 側にも更新版が届く
//!   - A が LeaveRoom → B 側に更新版が届く
//!   - Ping → Pong
//!
//! DEV_MODE では JWT バイパスで全員 dev@local 扱いなので、Hub から見ると同じ
//! プレイヤーが2回 join しているように見える(本番では別 email になる)。

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const URL: &str = "ws://127.0.0.1:8080/ws";

#[tokio::main]
async fn main() {
    println!("--- A connects, says Hello, creates room ---");
    let mut a = connect("alice").await;
    send_json(&mut a, r#"{"type":"hello","payload":{"display_name":"alice"}}"#).await;
    send_json(&mut a, r#"{"type":"create_room"}"#).await;
    drain(&mut a, "A", Duration::from_millis(300)).await;

    println!("\n--- B connects, joins same room (we'll get room_id from A's RoomState) ---");
    // 実際には A が受信した room_state.room_id を使うべきだが、
    // smoke テストでは print から目視で拾うか、room_id を最初の出力から手動で取る。
    // ここでは pings/pong だけ確認する。
    let mut b = connect("bob").await;
    send_json(&mut b, r#"{"type":"hello","payload":{"display_name":"bob"}}"#).await;
    send_json(&mut b, r#"{"type":"ping"}"#).await;
    drain(&mut b, "B", Duration::from_millis(300)).await;

    println!("\n--- A leaves, both disconnect ---");
    send_json(&mut a, r#"{"type":"leave_room"}"#).await;
    drain(&mut a, "A", Duration::from_millis(200)).await;
    let _ = a.close(None).await;
    let _ = b.close(None).await;
    println!("done");
}

async fn connect(label: &str) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let req = URL.into_client_request().expect("request");
    let (ws, _) = connect_async(req)
        .await
        .unwrap_or_else(|e| panic!("{label} connect: {e}"));
    println!("[{label}] connected");
    ws
}

async fn send_json<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>, json: &str)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    ws.send(Message::Text(json.into())).await.expect("send");
}

async fn drain<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    label: &str,
    for_ms: Duration,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let deadline = tokio::time::Instant::now() + for_ms;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => println!("[{label}] {}", t),
            Ok(Some(Ok(Message::Close(_)))) => {
                println!("[{label}] (closed)");
                break;
            }
            Ok(Some(Err(e))) => {
                println!("[{label}] err: {e}");
                break;
            }
            Ok(None) => break,
            Err(_) => break, // timeout
            _ => {}
        }
    }
}
