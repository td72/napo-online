//! Match 進行中にサーバが落ちて再起動 → 同じ DB から snapshot 復元 → JoinRoom で
//! 再接続して継続できる、ことを verify する統合テスト。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_core::card::Suit;
use napo_core::{Action, Bid, Match, Rules, Trump};
use napo_online::config::Config;
use napo_online::db::Db;
use napo_online::game_adapter::trump_to_wire;
use napo_online::hub::Hub;
use napo_online::protocol::{ClientMessage, RoomId, SeatAssignment, ServerMessage, WireAction};
use napo_online::room::RoomTimers;
use napo_online::{AppState, build_router};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Writer = SplitSink<Ws, Message>;
type Reader = SplitStream<Ws>;

const PLAYERS: &[&str] = &["alice", "bob", "carol", "dave", "eve"];
const RECV_TIMEOUT: Duration = Duration::from_secs(5);

struct ServerHandle {
    port: u16,
    task: JoinHandle<()>,
}

impl ServerHandle {
    async fn shutdown(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

async fn spawn_server_with_db(db: Db) -> ServerHandle {
    let cfg = Arc::new(Config::for_test());
    let timers = RoomTimers {
        t2_notify: cfg.t2_notify,
        t3_abort: cfg.t3_abort,
        t4_end: cfg.t4_end,
    };
    let hub = Hub::new(cfg.max_rooms, cfg.hands_per_match, db.clone(), timers);
    let state = AppState {
        cfg: cfg.clone(),
        metrics: napo_online::metrics::install_once(),
        verifier: None,
        hub,
        db,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = build_router(state);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    ServerHandle { port, task }
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

#[tokio::test]
async fn match_survives_server_restart_via_snapshot() {
    // 共有 DB(spawn_blocking 経由なので Arc<Mutex<Connection>> でプロセス内同 DB)
    let db = Db::open_in_memory().expect("open db");
    db.migrate().await.expect("migrate");

    // -- 第1サーバ --
    let s1 = spawn_server_with_db(db.clone()).await;
    let port = s1.port;

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
    send(
        &mut clients[0].0,
        &ClientMessage::CreateRoom {
            bidding_mode: napo_online::protocol::BiddingMode::Auto,
        },
    )
    .await;
    let room_id = wait_for_room_id(&mut clients[0].1).await;
    for (w, r) in clients.iter_mut().take(5).skip(1) {
        send(
            w,
            &ClientMessage::JoinRoom {
                room_id: room_id.clone(),
            },
        )
        .await;
        let _ = recv(r).await;
    }
    send(&mut clients[0].0, &ClientMessage::StartMatch).await;
    let mut seed: Option<u64> = None;
    let mut seats: Vec<SeatAssignment> = Vec::new();
    for (i, (_, r)) in clients.iter_mut().enumerate() {
        let m = wait_for(r, |m| matches!(m, ServerMessage::MatchStarted { .. })).await;
        if let ServerMessage::MatchStarted {
            seats: s,
            match_seed,
            ..
        } = m
            && i == 0
        {
            seed = Some(match_seed);
            seats = s;
        }
    }
    let seed = seed.expect("seed");
    let player_to_idx: HashMap<String, usize> = PLAYERS
        .iter()
        .enumerate()
        .map(|(i, n)| (format!("{n}@local"), i))
        .collect();
    let seat_to_idx: Vec<usize> = (0..5)
        .map(|seat| {
            let pid = &seats.iter().find(|s| s.seat == seat).unwrap().player_id;
            *player_to_idx.get(pid).unwrap()
        })
        .collect();

    // 数手だけ進めてからクラッシュさせる: 1人 Bid + 4人 Pass で DeclareAdjutant フェーズへ
    let mut local = Match::new(Rules::default(), seed);
    let bid = WireAction::Bid {
        tricks: 13,
        trump: trump_to_wire(Trump::Suit(Suit::Spade)),
    };
    let cur0 = local.current_hand().current_player().unwrap();
    local
        .apply(Action::Bid(Bid::new(13, Trump::Suit(Suit::Spade))))
        .unwrap();
    send(
        &mut clients[seat_to_idx[cur0]].0,
        &ClientMessage::SubmitAction { action: bid },
    )
    .await;
    // 4 pass
    for _ in 0..4 {
        let cur = local.current_hand().current_player().unwrap();
        local.apply(Action::Pass).unwrap();
        send(
            &mut clients[seat_to_idx[cur]].0,
            &ClientMessage::SubmitAction {
                action: WireAction::Pass,
            },
        )
        .await;
    }
    // 全部のメッセージを drain して状態を server に反映させる
    tokio::time::sleep(Duration::from_millis(300)).await;
    for (_, r) in clients.iter_mut() {
        while try_recv(r).await.is_some() {}
    }

    // -- クラッシュ → 全 WS を閉じてサーバ task を kill --
    for (mut w, _) in clients.drain(..) {
        let _ = w.send(Message::Close(None)).await;
    }
    s1.shutdown().await;

    // 同じ DB で別ポートで再起動
    let s2 = spawn_server_with_db(db.clone()).await;
    let port2 = s2.port;

    // alice (idx 0 = host) で再接続して JoinRoom → snapshot から復元
    let (mut a_w, mut a_r) = connect_player(port2, "alice").await;
    let _ = recv(&mut a_r).await; // welcome
    send(
        &mut a_w,
        &ClientMessage::Hello {
            display_name: "alice".to_string(),
        },
    )
    .await;
    send(
        &mut a_w,
        &ClientMessage::JoinRoom {
            room_id: room_id.clone(),
        },
    )
    .await;

    // 復元成功なら自分の Observation が来る(declarer なら adjutant_card 指名フェーズ等)
    let obs = wait_for(&mut a_r, |m| matches!(m, ServerMessage::Observation { .. })).await;
    if let ServerMessage::Observation { observation } = obs {
        // declarer は seat 0(最初に bid した人)= local.current_hand_started_at の seat
        // ともかく hand の枚数が 10 (まだ play してない)であることを確認
        // declarer は手札 + widow を扱うので場合により枚数が 10〜12 になる
        let hand_len = observation.hand.len();
        assert!(
            (8..=12).contains(&hand_len),
            "expected restored hand to have ~10 cards, got {hand_len}"
        );
    }

    let _ = a_w.send(Message::Close(None)).await;
    s2.shutdown().await;
}
