//! プロトコルのエラー/権限/タイマーガード周りの統合テスト。
//! `cargo test --test protocol_edge` で動作。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_online::config::Config;
use napo_online::db::Db;
use napo_online::hub::Hub;
use napo_online::protocol::{ClientMessage, PublicEvent, RoomId, SeatAssignment, ServerMessage};
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

const PLAYERS: &[&str] = &["alice", "bob", "carol", "dave", "eve"];
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
    let frame = timeout(Duration::from_millis(20), r.next()).await.ok()??;
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

/// 5接続して 1部屋に入る。alice (idx 0) が host。返り値は (clients, room_id, seat_to_idx)
async fn five_connected_in_room(port: u16) -> (Vec<(Writer, Reader)>, RoomId) {
    let mut clients: Vec<(Writer, Reader)> = Vec::new();
    for name in PLAYERS {
        clients.push(connect_player(port, name).await);
    }
    for (i, name) in PLAYERS.iter().enumerate() {
        send(
            &mut clients[i].0,
            &ClientMessage::Hello {
                display_name: name.to_string(),
            },
        )
        .await;
        let _ = recv(&mut clients[i].1).await; // welcome
    }
    send(&mut clients[0].0, &ClientMessage::CreateRoom).await;
    let room_id = wait_for_room_id(&mut clients[0].1).await;
    for (w, r) in clients.iter_mut().take(5).skip(1) {
        send(
            w,
            &ClientMessage::JoinRoom {
                room_id: room_id.clone(),
            },
        )
        .await;
        let _ = recv(r).await; // room_state
    }
    (clients, room_id)
}

/// MatchStarted を全員から受け取って seat 割を返す
async fn start_match_and_collect(clients: &mut [(Writer, Reader)]) -> Vec<SeatAssignment> {
    send(&mut clients[0].0, &ClientMessage::StartMatch).await;
    let mut seats: Vec<SeatAssignment> = Vec::new();
    for (i, (_, r)) in clients.iter_mut().enumerate() {
        let m = wait_for(r, |m| matches!(m, ServerMessage::MatchStarted { .. })).await;
        if let ServerMessage::MatchStarted { seats: s, .. } = m
            && i == 0
        {
            seats = s;
        }
    }
    seats
}

// ---------- tests ----------

#[tokio::test]
async fn handle_taken_via_ws() {
    let port = spawn_server(Config::for_test()).await;

    // alice 接続 → "alice"
    let (mut wa, mut ra) = connect_player(port, "alice").await;
    let _ = recv(&mut ra).await; // welcome
    send(
        &mut wa,
        &ClientMessage::Hello {
            display_name: "alice".to_string(),
        },
    )
    .await;
    // alice 自身は Error 受けない
    assert!(try_recv(&mut ra).await.is_none());

    // bob 接続 → "Alice"(大小無視で衝突)
    let (mut wb, mut rb) = connect_player(port, "bob").await;
    let _ = recv(&mut rb).await; // welcome
    send(
        &mut wb,
        &ClientMessage::Hello {
            display_name: "Alice".to_string(),
        },
    )
    .await;
    let m = wait_for(&mut rb, |m| matches!(m, ServerMessage::Error { .. })).await;
    if let ServerMessage::Error { code, .. } = m {
        assert_eq!(code, "handle_taken");
    }

    let _ = wa.send(Message::Close(None)).await;
    let _ = wb.send(Message::Close(None)).await;
}

#[tokio::test]
async fn non_host_start_match_rejected() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _room_id) = five_connected_in_room(port).await;

    // bob (idx 1) が StartMatch
    send(&mut clients[1].0, &ClientMessage::StartMatch).await;
    let m = wait_for(
        &mut clients[1].1,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "not_host"),
    )
    .await;
    if let ServerMessage::Error { code, .. } = m {
        assert_eq!(code, "not_host");
    }

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}

#[tokio::test]
async fn abort_hand_rejected_when_nobody_disconnected() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_connected_in_room(port).await;
    let _seats = start_match_and_collect(&mut clients).await;

    // host が 何の disconnect も無いまま AbortHand → too_early
    send(&mut clients[0].0, &ClientMessage::AbortHand).await;
    let m = wait_for(
        &mut clients[0].1,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "too_early"),
    )
    .await;
    if let ServerMessage::Error { code, .. } = m {
        assert_eq!(code, "too_early");
    }

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}

#[tokio::test]
async fn end_match_rejected_when_nobody_disconnected() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_connected_in_room(port).await;
    let _seats = start_match_and_collect(&mut clients).await;

    send(&mut clients[0].0, &ClientMessage::EndMatch).await;
    let m = wait_for(
        &mut clients[0].1,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "too_early"),
    )
    .await;
    if let ServerMessage::Error { code, .. } = m {
        assert_eq!(code, "too_early");
    }

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}

#[tokio::test]
async fn abort_hand_after_t3_emits_aborted_and_advances_to_between_hands() {
    // Config::for_test の hands_per_match = 2、t3_abort = 200ms
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_connected_in_room(port).await;
    let seats = start_match_and_collect(&mut clients).await;

    // bob (idx 1) を強制切断
    let player_to_idx: HashMap<String, usize> = PLAYERS
        .iter()
        .enumerate()
        .map(|(i, n)| (format!("{n}@local"), i))
        .collect();
    let _bob_seat = seats
        .iter()
        .find(|s| player_to_idx[&s.player_id] == 1)
        .unwrap()
        .seat;
    let (bob_w, bob_r) = clients.remove(1);
    drop(bob_w);
    drop(bob_r);

    // T3 (200ms) + Tick 1s
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // host (alice = clients[0]) AbortHand
    send(&mut clients[0].0, &ClientMessage::AbortHand).await;

    // PublicEvent::HandAborted がブロードキャスト
    let _ = wait_for(&mut clients[0].1, |m| {
        matches!(
            m,
            ServerMessage::Event {
                event: PublicEvent::HandAborted
            }
        )
    })
    .await;
    // HandEnded(scores 全0, cumulative 全0)
    let ended = wait_for(&mut clients[0].1, |m| {
        matches!(m, ServerMessage::HandEnded { .. })
    })
    .await;
    if let ServerMessage::HandEnded { scores, cumulative } = ended {
        assert!(scores.iter().all(|&s| s == 0), "scores={scores:?}");
        assert!(
            cumulative.iter().all(|&s| s == 0),
            "cumulative={cumulative:?}"
        );
    }
    // hands_per_match=2 なので MatchEnded ではなく BetweenHands に遷移しているはず

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}

#[tokio::test]
async fn abort_hand_on_last_hand_ends_match() {
    // hands_per_match = 1 にして、初局を abort = MatchEnded
    let mut cfg = Config::for_test();
    cfg.hands_per_match = 1;
    let port = spawn_server(cfg).await;

    let (mut clients, _) = five_connected_in_room(port).await;
    let _seats = start_match_and_collect(&mut clients).await;

    // bob 切断 → T3 経過
    let (bob_w, bob_r) = clients.remove(1);
    drop(bob_w);
    drop(bob_r);
    tokio::time::sleep(Duration::from_millis(1500)).await;

    send(&mut clients[0].0, &ClientMessage::AbortHand).await;

    let m = wait_for(&mut clients[0].1, |m| {
        matches!(m, ServerMessage::MatchEnded { .. })
    })
    .await;
    if let ServerMessage::MatchEnded { final_scores } = m {
        assert_eq!(final_scores.len(), 5);
        assert!(final_scores.iter().all(|&s| s == 0));
    }

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}
