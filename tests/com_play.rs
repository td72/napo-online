//! 1 人 + 4 COM で部屋を作ってマッチ開始まで通すスモーク。
//! COM driver(`Room::drive_coms`)が起動して例外を吐かないこと、AddCom が member を
//! 増やすことを確認する。1 局完走テストはローカル Match との同期がややこしいので、
//! e2e (Playwright) 側か別 spec で対応。

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_online::config::Config;
use napo_online::db::Db;
use napo_online::hub::Hub;
use napo_online::protocol::{ClientMessage, ServerMessage};
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

const RECV_TIMEOUT: Duration = Duration::from_secs(10);

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

async fn send_msg(w: &mut Writer, m: &ClientMessage) {
    let s = serde_json::to_string(m).unwrap();
    w.send(Message::Text(s.into())).await.unwrap();
}

async fn recv_msg(r: &mut Reader) -> ServerMessage {
    loop {
        let frame = timeout(RECV_TIMEOUT, r.next()).await.expect("recv timeout");
        match frame {
            Some(Ok(Message::Text(t))) => return serde_json::from_str(&t).unwrap(),
            Some(Ok(Message::Pong(_))) | Some(Ok(Message::Ping(_))) => continue,
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

async fn drain_until<F: Fn(&ServerMessage) -> bool>(r: &mut Reader, pred: F) -> ServerMessage {
    loop {
        let m = recv_msg(r).await;
        if pred(&m) {
            return m;
        }
    }
}

#[tokio::test]
async fn host_can_add_four_coms_and_start_match() {
    let port = spawn_test_server().await;

    let url = format!("ws://127.0.0.1:{port}/ws?dev_email=alice@local");
    let req = url.into_client_request().unwrap();
    let (ws, _) = connect_async(req).await.expect("connect");
    let (mut w, mut r) = ws.split();

    send_msg(
        &mut w,
        &ClientMessage::Hello {
            display_name: "alice".into(),
        },
    )
    .await;
    drain_until(&mut r, |m| matches!(m, ServerMessage::Welcome { .. })).await;

    send_msg(
        &mut w,
        &ClientMessage::CreateRoom {
            bidding_mode: napo_online::protocol::BiddingMode::Auto,
        },
    )
    .await;
    drain_until(&mut r, |m| matches!(m, ServerMessage::RoomState { .. })).await;

    // COM × 4 追加 → 各 broadcast を待つ
    for i in 1..=4 {
        send_msg(
            &mut w,
            &ClientMessage::AddCom {
                display_name: format!("COM-{i}"),
            },
        )
        .await;
        let view = drain_until(&mut r, |m| match m {
            ServerMessage::RoomState { state } => state.players.len() == 1 + i,
            _ => false,
        })
        .await;
        if let ServerMessage::RoomState { state } = view {
            // is_com フラグが正しく出てる
            let coms: Vec<_> = state.players.iter().filter(|p| p.is_com).collect();
            assert_eq!(coms.len(), i, "expected {i} COMs");
        }
    }

    // start_match
    send_msg(&mut w, &ClientMessage::StartMatch).await;
    let started = drain_until(&mut r, |m| matches!(m, ServerMessage::MatchStarted { .. })).await;
    if let ServerMessage::MatchStarted { seats, .. } = started {
        assert_eq!(seats.len(), 5);
        assert!(seats.iter().any(|s| s.player_id == "alice@local"));
    }

    // 4 COM が起動するので bidding round で順次 pass する。
    // alice の to_act を待つ間に少なくとも 1 個の Event が来るはず(BidPass, etc.)。
    // ここでは event が壊れていないことだけ確認(panic などしていない)。
    timeout(Duration::from_secs(8), async {
        loop {
            let m = recv_msg(&mut r).await;
            if let ServerMessage::Observation { observation } = m
                && observation.to_act == Some(observation.player)
            {
                // alice の番が来た = COM 順送りが少なくとも 1 周進んだ
                break;
            }
        }
    })
    .await
    .expect("alice never got to_act in 8s");

    let _ = w.send(Message::Close(None)).await;
}
