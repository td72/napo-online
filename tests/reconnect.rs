//! 切断 → Suspended → T2 通知 → 再接続 → 続行 まで通す統合テスト。
//! `cargo test --test reconnect` で 6〜10s 程度。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_core::{Action, Match, Phase, Rules};
use napo_online::config::Config;
use napo_online::db::Db;
use napo_online::game_adapter::{card_to_wire, trump_to_wire};
use napo_online::hub::Hub;
use napo_online::protocol::{
    ClientMessage, PublicEvent, RoomId, SeatAssignment, ServerMessage, WireAction,
};
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

async fn spawn_test_server() -> u16 {
    let cfg = Arc::new(Config::for_test());
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

#[tokio::test]
async fn disconnect_then_reconnect_and_finish_hand() {
    let port = spawn_test_server().await;

    let mut clients: Vec<(Writer, Reader)> = Vec::new();
    for name in PLAYERS {
        clients.push(connect_player(port, name).await);
    }

    // Hello, drain Welcome
    for (i, name) in PLAYERS.iter().enumerate() {
        send(
            &mut clients[i].0,
            &ClientMessage::Hello {
                display_name: name.to_string(),
            },
        )
        .await;
        let _ = recv(&mut clients[i].1).await;
    }

    // alice creates room, others join
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

    // start match
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
    // 逆引き: 接続 idx → seat
    let mut idx_to_seat = [0usize; 5];
    for (seat, idx) in seat_to_idx.iter().enumerate() {
        idx_to_seat[*idx] = seat;
    }

    // bob (idx 1) を強制切断
    let bob_seat = idx_to_seat[1];
    let (bob_writer, _bob_reader) = clients.remove(1);
    drop(bob_writer);
    drop(_bob_reader);

    // T2 (50ms) 経過 + Tick (1s) を待つ → 残り 4人に PlayerDisconnected が届く
    // Tick interval は 1s なので 1.5s 待つ
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let mut got_disconnect = false;
    for (_, r) in clients.iter_mut() {
        while let Some(m) = try_recv(r).await {
            if let ServerMessage::PlayerDisconnected { seat, .. } = m
                && seat == bob_seat
            {
                got_disconnect = true;
            }
        }
    }
    assert!(
        got_disconnect,
        "expected PlayerDisconnected for seat {bob_seat} after T2"
    );

    // bob 再接続
    let (mut bob_w, mut bob_r) = connect_player(port, "bob").await;
    let _ = recv(&mut bob_r).await; // welcome
    // Hello → 既存 handle が拾われる(再接続でも display_name 上書きできる)
    send(
        &mut bob_w,
        &ClientMessage::Hello {
            display_name: "bob".to_string(),
        },
    )
    .await;
    // JoinRoom で再接続: server が「既存 seated」と判定して Online に戻し、Observation 送信、
    // 他に PlayerReconnected をブロードキャスト
    send(
        &mut bob_w,
        &ClientMessage::JoinRoom {
            room_id: room_id.clone(),
        },
    )
    .await;

    // bob 自身が Observation を受け取るのを待つ
    let _ = wait_for(&mut bob_r, |m| {
        matches!(m, ServerMessage::Observation { .. })
    })
    .await;

    // 他のプレイヤーが PlayerReconnected を受け取る
    let mut got_reconnect = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline && !got_reconnect {
        for (_, r) in clients.iter_mut() {
            while let Some(m) = try_recv(r).await {
                if let ServerMessage::PlayerReconnected { seat } = m
                    && seat == bob_seat
                {
                    got_reconnect = true;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        got_reconnect,
        "expected PlayerReconnected for seat {bob_seat} after rejoin"
    );

    // bob を clients に戻す(seat_to_idx の bob 位置に)
    clients.insert(1, (bob_w, bob_r));

    // 1局を完走させる
    let mut local = Match::new(Rules::default(), seed);
    let mut hand_ended = false;
    let mut iters = 0;
    while !hand_ended && iters < 200 {
        iters += 1;
        let cur_seat = local.current_hand().current_player().expect("cur");
        let action = pick_action(local.current_hand());
        let wire = action_to_wire(&action);
        local.apply(action.clone()).expect("local apply");
        send(
            &mut clients[seat_to_idx[cur_seat]].0,
            &ClientMessage::SubmitAction {
                action: wire.clone(),
            },
        )
        .await;

        for (_, r) in clients.iter_mut() {
            while let Some(msg) = try_recv(r).await {
                if let ServerMessage::Event {
                    event: PublicEvent::HandEnded { .. },
                } = msg
                {
                    hand_ended = true;
                }
            }
        }
    }

    assert!(
        hand_ended,
        "expected HandEnded within {iters} iterations after reconnect"
    );

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}

fn pick_action(g: &napo_core::Game) -> Action {
    let actions = g.legal_actions();
    if let Phase::Bidding {
        current_bid: None, ..
    } = g.phase()
        && let Some(bid) = actions.iter().find(|a| matches!(a, Action::Bid(_)))
    {
        return bid.clone();
    }
    actions.into_iter().next().expect("legal actions empty")
}

fn action_to_wire(a: &Action) -> WireAction {
    match a {
        Action::Bid(b) => WireAction::Bid {
            tricks: b.tricks,
            trump: trump_to_wire(b.trump),
        },
        Action::Pass => WireAction::Pass,
        Action::DeclareAdjutant(c) => WireAction::DeclareAdjutant {
            card: card_to_wire(*c),
        },
        Action::ExchangeWidow { discard } => WireAction::ExchangeWidow {
            discard: discard.iter().copied().map(card_to_wire).collect(),
        },
        Action::PlayCard(c) => WireAction::PlayCard {
            card: card_to_wire(*c),
        },
    }
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

async fn wait_for_room_id(r: &mut Reader) -> RoomId {
    let m = wait_for(r, |m| matches!(m, ServerMessage::RoomState { .. })).await;
    if let ServerMessage::RoomState { state } = m {
        return state.room_id;
    }
    unreachable!()
}

async fn wait_for(r: &mut Reader, pred: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
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
