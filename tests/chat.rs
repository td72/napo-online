//! チャット機能の統合テスト。
//! `cargo test --test chat` で動作。

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_online::config::Config;
use napo_online::db::Db;
use napo_online::hub::Hub;
use napo_online::protocol::{ChatEntry, ClientMessage, RoomId, ServerMessage};
use napo_online::room::RoomTimers;
use napo_online::{AppState, build_router};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Writer = SplitSink<Ws, Message>;
type Reader = SplitStream<Ws>;

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

async fn spawn_server(cfg: Config) -> u16 {
    let cfg = Arc::new(cfg);
    let db = Db::open_in_memory().expect("open db");
    db.migrate().await.expect("migrate");
    let timers = RoomTimers {
        t2_notify: cfg.t2_notify,
        t3_abort: cfg.t3_abort,
        t4_end: cfg.t4_end,
    };
    let hub = Hub::new(cfg.max_rooms, cfg.hands_per_match, db.clone(), timers);
    let state = AppState {
        cfg: cfg.clone(),
        verifier: None,
        hub,
        db,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = build_router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    port
}

async fn connect_player(port: u16, name: &str) -> (Writer, Reader) {
    let url = format!("ws://127.0.0.1:{port}/ws?dev_email={name}@local");
    let req = url.into_client_request().unwrap();
    let (ws, _) = connect_async(req).await.expect("connect");
    ws.split()
}

async fn send(w: &mut Writer, m: &ClientMessage) {
    let s = serde_json::to_string(m).unwrap();
    w.send(Message::Text(s.into())).await.expect("send");
}

async fn recv(r: &mut Reader) -> ServerMessage {
    let frame = timeout(RECV_TIMEOUT, r.next())
        .await
        .expect("recv timeout")
        .expect("ws closed")
        .expect("ws err");
    match frame {
        Message::Text(t) => serde_json::from_str(t.as_ref()).unwrap(),
        other => panic!("expected text, got {other:?}"),
    }
}

async fn try_recv(r: &mut Reader) -> Option<ServerMessage> {
    let frame = timeout(Duration::from_millis(50), r.next()).await.ok()??;
    let frame = frame.ok()?;
    let text = match frame {
        Message::Text(t) => t,
        _ => return None,
    };
    serde_json::from_str(text.as_ref()).ok()
}

async fn wait_for(r: &mut Reader, pred: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("wait_for timed out");
        }
        let frame = timeout(remaining, r.next())
            .await
            .expect("wait_for timeout")
            .expect("ws closed")
            .expect("ws err");
        if let Message::Text(t) = frame
            && let Ok(m) = serde_json::from_str::<ServerMessage>(t.as_ref())
            && pred(&m)
        {
            return m;
        }
    }
}

async fn wait_for_room_id(r: &mut Reader) -> RoomId {
    let m = wait_for(r, |m| matches!(m, ServerMessage::RoomState { .. })).await;
    if let ServerMessage::RoomState { state } = m {
        return state.room_id;
    }
    unreachable!()
}

async fn hello_create_room(port: u16, name: &str) -> (Writer, Reader, RoomId) {
    let (mut w, mut r) = connect_player(port, name).await;
    let _ = recv(&mut r).await; // welcome
    send(
        &mut w,
        &ClientMessage::Hello {
            display_name: name.to_string(),
        },
    )
    .await;
    send(&mut w, &ClientMessage::CreateRoom).await;
    let room_id = wait_for_room_id(&mut r).await;
    (w, r, room_id)
}

async fn hello_join_room(port: u16, name: &str, room_id: &RoomId) -> (Writer, Reader) {
    let (mut w, mut r) = connect_player(port, name).await;
    let _ = recv(&mut r).await; // welcome
    send(
        &mut w,
        &ClientMessage::Hello {
            display_name: name.to_string(),
        },
    )
    .await;
    send(
        &mut w,
        &ClientMessage::JoinRoom {
            room_id: room_id.clone(),
        },
    )
    .await;
    let _ = wait_for(&mut r, |m| matches!(m, ServerMessage::RoomState { .. })).await;
    (w, r)
}

async fn wait_for_chat_with_text(r: &mut Reader, expected: &str) -> ChatEntry {
    let m = wait_for(r, |m| match m {
        ServerMessage::Chat { entry } => entry.text == expected,
        _ => false,
    })
    .await;
    if let ServerMessage::Chat { entry } = m {
        return entry;
    }
    unreachable!()
}

// ---------- tests ----------

#[tokio::test]
async fn chat_broadcasts_to_all_room_members() {
    let port = spawn_server(Config::for_test()).await;
    let (mut wa, mut ra, room_id) = hello_create_room(port, "alice").await;
    let (_wb, mut rb) = hello_join_room(port, "bob", &room_id).await;
    // alice 側にも bob の join に伴う RoomState が飛ぶ。続く Chat を捕まえるためにここで drain は不要(wait_for_chat_with_text が拾う)。

    send(
        &mut wa,
        &ClientMessage::Chat {
            text: "hello world".to_string(),
        },
    )
    .await;

    let from_a = wait_for_chat_with_text(&mut ra, "hello world").await;
    let from_b = wait_for_chat_with_text(&mut rb, "hello world").await;
    assert_eq!(from_a.text, "hello world");
    assert_eq!(from_a.sender_id, "alice@local");
    assert_eq!(from_a.sender_name, "alice");
    assert_eq!(from_a.seq, 1);
    assert_eq!(from_b.seq, 1);
    assert_eq!(from_b.sender_id, "alice@local");
}

#[tokio::test]
async fn chat_backlog_on_join() {
    let port = spawn_server(Config::for_test()).await;
    let (mut wa, mut ra, room_id) = hello_create_room(port, "alice").await;

    // alice 1人で 3 件チャット送信
    for i in 1..=3 {
        send(
            &mut wa,
            &ClientMessage::Chat {
                text: format!("msg{i}"),
            },
        )
        .await;
        let _ = wait_for_chat_with_text(&mut ra, &format!("msg{i}")).await;
    }

    // bob を手動で接続(hello_join_room は RoomState を consume するため backlog を取り逃がす)
    let (mut wb, mut rb) = connect_player(port, "bob").await;
    let _ = recv(&mut rb).await; // welcome
    send(
        &mut wb,
        &ClientMessage::Hello {
            display_name: "bob".to_string(),
        },
    )
    .await;
    send(
        &mut wb,
        &ClientMessage::JoinRoom {
            room_id: room_id.clone(),
        },
    )
    .await;

    // ChatBacklog を待つ(順序: 実装上 backlog → RoomState の順に来る想定だが念のため wait_for で拾う)
    let m = wait_for(&mut rb, |m| matches!(m, ServerMessage::ChatBacklog { .. })).await;
    if let ServerMessage::ChatBacklog { entries } = m {
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].seq, 1);
        assert_eq!(entries[0].text, "msg1");
        assert_eq!(entries[2].seq, 3);
        assert_eq!(entries[2].text, "msg3");
    }
}

#[tokio::test]
async fn chat_empty_or_whitespace_rejected() {
    let port = spawn_server(Config::for_test()).await;
    let (mut wa, mut ra, room_id) = hello_create_room(port, "alice").await;
    let (_wb, mut rb) = hello_join_room(port, "bob", &room_id).await;

    // 空文字 / 空白だけは broadcast されない
    send(
        &mut wa,
        &ClientMessage::Chat {
            text: "".to_string(),
        },
    )
    .await;
    send(
        &mut wa,
        &ClientMessage::Chat {
            text: "    \t  ".to_string(),
        },
    )
    .await;

    // 直後に正規 chat を送って、それを受け取った時点で空文字 chat が来てないことを確認
    send(
        &mut wa,
        &ClientMessage::Chat {
            text: "real".to_string(),
        },
    )
    .await;

    let entry_a = wait_for_chat_with_text(&mut ra, "real").await;
    let entry_b = wait_for_chat_with_text(&mut rb, "real").await;
    // 空文字を弾いているので、最初の chat = seq 1。
    assert_eq!(entry_a.seq, 1);
    assert_eq!(entry_b.seq, 1);

    // 念のためその後余分な chat が来ないことを確認
    assert!(try_recv(&mut ra).await.is_none());
    assert!(try_recv(&mut rb).await.is_none());
}

#[tokio::test]
async fn chat_long_text_is_truncated() {
    let port = spawn_server(Config::for_test()).await;
    let (mut wa, mut ra, _room_id) = hello_create_room(port, "alice").await;

    let long = "x".repeat(600);
    send(&mut wa, &ClientMessage::Chat { text: long.clone() }).await;
    let m = wait_for(&mut ra, |m| matches!(m, ServerMessage::Chat { .. })).await;
    if let ServerMessage::Chat { entry } = m {
        assert_eq!(entry.text.chars().count(), 500);
    }
}

#[tokio::test]
async fn chat_scrollback_http_returns_older() {
    let port = spawn_server(Config::for_test()).await;
    let (mut wa, mut ra, room_id) = hello_create_room(port, "alice").await;

    // 150 件送る
    for i in 1..=150 {
        send(
            &mut wa,
            &ClientMessage::Chat {
                text: format!("m{i}"),
            },
        )
        .await;
        let _ = wait_for_chat_with_text(&mut ra, &format!("m{i}")).await;
    }

    // before_seq=51 → seq 1..50 が来る(古い→新しい順)
    let url = format!(
        "http://127.0.0.1:{port}/api/rooms/{room_id}/chat?dev_email=alice@local&before_seq=51&limit=100"
    );
    let resp: serde_json::Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
    let entries = resp["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 50);
    assert_eq!(entries[0]["seq"], 1);
    assert_eq!(entries[0]["text"], "m1");
    assert_eq!(entries[49]["seq"], 50);
    assert_eq!(entries[49]["text"], "m50");

    // 何も指定しないと最新 100 件(seq 51..150)
    let url = format!("http://127.0.0.1:{port}/api/rooms/{room_id}/chat?dev_email=alice@local");
    let resp: serde_json::Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
    let entries = resp["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 100);
    assert_eq!(entries[0]["seq"], 51);
    assert_eq!(entries[99]["seq"], 150);
}
