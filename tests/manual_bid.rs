//! manual bidding mode の統合テスト。

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_online::config::Config;
use napo_online::db::Db;
use napo_online::hub::Hub;
use napo_online::protocol::{
    BiddingMode, ClientMessage, PublicEvent, RoomId, ServerMessage, WireBid, WireTrump,
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

async fn connect(port: u16, name: &str) -> (Writer, Reader) {
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

async fn five_in_room_manual(port: u16) -> (Vec<(Writer, Reader)>, RoomId) {
    let mut clients: Vec<(Writer, Reader)> = Vec::new();
    for name in PLAYERS {
        clients.push(connect(port, name).await);
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
            bidding_mode: BiddingMode::Manual,
        },
    )
    .await;
    let m = wait_for(&mut clients[0].1, |m| {
        matches!(m, ServerMessage::RoomState { .. })
    })
    .await;
    let room_id = if let ServerMessage::RoomState { state } = m {
        assert_eq!(state.bidding_mode, BiddingMode::Manual);
        state.room_id
    } else {
        unreachable!()
    };
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
    (clients, room_id)
}

async fn start_match(clients: &mut [(Writer, Reader)]) {
    send(&mut clients[0].0, &ClientMessage::StartMatch).await;
    for (_, r) in clients.iter_mut() {
        let _ = wait_for(r, |m| matches!(m, ServerMessage::MatchStarted { .. })).await;
    }
}

#[tokio::test]
async fn manual_bid_makes_declarer_and_skips_to_adjutant_phase() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_in_room_manual(port).await;
    start_match(&mut clients).await;

    // alice (host, seat 0) が「bob (seat 1) が 15 ♠ で立つ」と決定
    send(
        &mut clients[0].0,
        &ClientMessage::ManualBid {
            declarer_seat: 1,
            bid: WireBid {
                tricks: 15,
                trump: WireTrump::S,
            },
        },
    )
    .await;

    // NapoleonDecided event が全員に流れるはず
    for (_, r) in clients.iter_mut() {
        let m = wait_for(r, |m| {
            matches!(
                m,
                ServerMessage::Event {
                    event: PublicEvent::NapoleonDecided { .. }
                }
            )
        })
        .await;
        if let ServerMessage::Event {
            event: PublicEvent::NapoleonDecided { seat, bid },
        } = m
        {
            assert_eq!(seat, 1);
            assert_eq!(bid.tricks, 15);
        }
    }
}

#[tokio::test]
async fn manual_bid_rejected_from_non_host() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_in_room_manual(port).await;
    start_match(&mut clients).await;

    // bob (non-host) が ManualBid を送る → not_host
    send(
        &mut clients[1].0,
        &ClientMessage::ManualBid {
            declarer_seat: 1,
            bid: WireBid {
                tricks: 15,
                trump: WireTrump::S,
            },
        },
    )
    .await;
    let m = wait_for(
        &mut clients[1].1,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "not_host"),
    )
    .await;
    if let ServerMessage::Error { code, .. } = m {
        assert_eq!(code, "not_host");
    }
}

#[tokio::test]
async fn submit_bid_rejected_in_manual_mode() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_in_room_manual(port).await;
    start_match(&mut clients).await;

    // 最初の bidder が Pass を送ろうとしても manual モードなので reject。
    // (どの seat が first bidder か napo-core のシードに依存するので、5 人とも試して
    //  少なくとも 1 つは "manual_bid_only" を受け取ることを確認する。)
    for (w, _) in clients.iter_mut() {
        send(
            w,
            &ClientMessage::SubmitAction {
                action: napo_online::protocol::WireAction::Pass,
            },
        )
        .await;
    }
    // それぞれの sender に manual_bid_only Error が返るはず(他の broadcast 等を
    // スキップしつつ 500ms 以内に拾えれば OK)。少なくとも 1 つ取れれば成功。
    let mut got = false;
    'outer: for (_, r) in clients.iter_mut() {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let Ok(Some(frame)) = timeout(remaining, r.next()).await else {
                break;
            };
            let Ok(Message::Text(t)) = frame else {
                continue;
            };
            let Ok(msg) = serde_json::from_str::<ServerMessage>(t.as_ref()) else {
                continue;
            };
            if let ServerMessage::Error { code, .. } = msg
                && code == "manual_bid_only"
            {
                got = true;
                break 'outer;
            }
        }
    }
    assert!(got, "expected at least one manual_bid_only error");
}

#[tokio::test]
async fn manual_bid_rejected_outside_bidding_phase() {
    let port = spawn_server(Config::for_test()).await;
    let (mut clients, _) = five_in_room_manual(port).await;
    start_match(&mut clients).await;

    // 1 回目 ManualBid → bidding 通過(DeclareAdjutant phase に)
    send(
        &mut clients[0].0,
        &ClientMessage::ManualBid {
            declarer_seat: 1,
            bid: WireBid {
                tricks: 15,
                trump: WireTrump::S,
            },
        },
    )
    .await;
    let _ = wait_for(&mut clients[0].1, |m| {
        matches!(
            m,
            ServerMessage::Event {
                event: PublicEvent::NapoleonDecided { .. }
            }
        )
    })
    .await;

    // 2 回目を送ると DeclareAdjutant phase なので wrong_phase
    send(
        &mut clients[0].0,
        &ClientMessage::ManualBid {
            declarer_seat: 2,
            bid: WireBid {
                tricks: 16,
                trump: WireTrump::H,
            },
        },
    )
    .await;
    let m = wait_for(
        &mut clients[0].1,
        |m| matches!(m, ServerMessage::Error { code, .. } if code == "wrong_phase"),
    )
    .await;
    if let ServerMessage::Error { code, .. } = m {
        assert_eq!(code, "wrong_phase");
    }
}
