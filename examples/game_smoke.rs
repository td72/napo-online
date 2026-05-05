//! Phase 3 サブステップ D: 5接続を立てて 1局を完走させる smoke テスト。
//!
//! 1) napo-online を `NAPO_DEV_MODE=true cargo run` で起動
//! 2) `cargo run --example game_smoke` で実行
//!
//! 5人(alice/bob/carol/dave/eve)が DEV_MODE の `?dev_email=X@local` で別 identity として接続。
//! alice が部屋を作って StartMatch、各プレイヤーは server から受信した `match_seed` で
//! 自分のローカル `napo_core::Match` を生成し、`legal_actions()[0]` を選んで SubmitAction。
//! HandEnded を受信したら成功。

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use napo_core::{Action, Match, Phase, Rules};
use napo_online::game_adapter::{card_to_wire, trump_to_wire};
use napo_online::protocol::{
    ClientMessage, PublicEvent, RoomId, SeatAssignment, ServerMessage, WireAction,
};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Writer = SplitSink<Ws, Message>;
type Reader = SplitStream<Ws>;

const PLAYERS: &[&str] = &["alice", "bob", "carol", "dave", "eve"];
const RECV_TIMEOUT: Duration = Duration::from_secs(3);

#[tokio::main]
async fn main() {
    let mut clients: Vec<(Writer, Reader)> = Vec::new();
    for name in PLAYERS {
        let url = format!("ws://127.0.0.1:8080/ws?dev_email={name}@local");
        let req = url.into_client_request().expect("request");
        let (ws, _) = connect_async(req)
            .await
            .unwrap_or_else(|e| panic!("{name} connect: {e}"));
        let (w, r) = ws.split();
        clients.push((w, r));
        println!("[{name}] connected");
    }

    // Hello + drain Welcome
    for (idx, name) in PLAYERS.iter().enumerate() {
        send_json(
            &mut clients[idx].0,
            &ClientMessage::Hello {
                display_name: name.to_string(),
            },
        )
        .await;
        let _ = recv_one(&mut clients[idx].1).await; // welcome
    }

    // alice (idx 0): create room → grab room_id from RoomState
    send_json(&mut clients[0].0, &ClientMessage::CreateRoom).await;
    let room_id = wait_for_room_id(&mut clients[0].1).await;
    println!("alice created room {room_id}");

    // bob..eve: join the same room
    for (w, r) in clients.iter_mut().take(5).skip(1) {
        send_json(
            w,
            &ClientMessage::JoinRoom {
                room_id: room_id.clone(),
            },
        )
        .await;
        let _ = recv_one(r).await; // room_state
    }

    // alice: start match
    send_json(&mut clients[0].0, &ClientMessage::StartMatch).await;

    // 各プレイヤーが MatchStarted を受信するのを待つ
    let mut match_seed: Option<u64> = None;
    let mut seats: Vec<SeatAssignment> = Vec::new();
    for (i, (_, r)) in clients.iter_mut().enumerate() {
        let started = wait_for(r, |m| matches!(m, ServerMessage::MatchStarted { .. })).await;
        if let ServerMessage::MatchStarted {
            seats: s,
            match_seed: seed,
            ..
        } = started
            && i == 0
        {
            match_seed = Some(seed);
            seats = s;
        }
    }
    let seed = match_seed.expect("match_seed missing");
    println!("MatchStarted: seed={seed}, seats={seats:?}");

    // seat → player_id, player_id → connection idx を作る
    let player_to_idx: HashMap<String, usize> = PLAYERS
        .iter()
        .enumerate()
        .map(|(i, name)| (format!("{name}@local"), i))
        .collect();
    let seat_to_idx: Vec<usize> = (0..5)
        .map(|seat| {
            let pid = &seats.iter().find(|s| s.seat == seat).unwrap().player_id;
            *player_to_idx.get(pid).expect("player not found")
        })
        .collect();

    // ローカル Match を構築(server と同じ seed/rules で deal が一致)
    let mut local = Match::new(Rules::default(), seed);
    let mut hand_ended = false;

    // 1局完走
    while !hand_ended {
        let cur_seat = local
            .current_hand()
            .current_player()
            .expect("current_player");
        let action = pick_action(local.current_hand());
        let wire = action_to_wire(&action);
        local.apply(action.clone()).expect("local apply");

        let conn_idx = seat_to_idx[cur_seat];
        send_json(
            &mut clients[conn_idx].0,
            &ClientMessage::SubmitAction {
                action: wire.clone(),
            },
        )
        .await;
        println!("[seat {cur_seat}] -> {wire:?}");

        // 全 connection から残ってる応答を吸い出して、HandEnded を検出
        for (_, r) in clients.iter_mut() {
            while let Some(msg) = try_recv(r).await {
                if let ServerMessage::Event {
                    event: PublicEvent::HandEnded { scores },
                } = &msg
                {
                    println!("HandEnded: {scores:?}");
                    hand_ended = true;
                }
            }
        }
    }

    // HandEnded(top-level message)も受け取る
    for (i, (_, r)) in clients.iter_mut().enumerate() {
        if let Some(msg) = try_recv(r).await {
            println!("[{}] final tail: {msg:?}", PLAYERS[i]);
        }
    }

    println!("=== smoke test ok ===");

    for (mut w, _) in clients {
        let _ = w.send(Message::Close(None)).await;
    }
}

fn pick_action(g: &napo_core::Game) -> Action {
    let actions = g.legal_actions();
    // Bidding 中で current_bid が無ければ最初の Bid を選んで配り直しを止める
    if let Phase::Bidding {
        current_bid: None, ..
    } = g.phase()
        && let Some(bid) = actions.iter().find(|a| matches!(a, Action::Bid(_)))
    {
        return bid.clone();
    }
    actions.into_iter().next().expect("no legal actions")
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

async fn send_json(w: &mut Writer, msg: &ClientMessage) {
    let s = serde_json::to_string(msg).expect("serialize");
    w.send(Message::Text(s.into())).await.expect("ws send");
}

async fn recv_one(r: &mut Reader) -> ServerMessage {
    let frame = timeout(RECV_TIMEOUT, r.next())
        .await
        .expect("recv timeout")
        .expect("ws closed")
        .expect("ws err");
    let text = match frame {
        Message::Text(t) => t,
        other => panic!("expected text frame, got {other:?}"),
    };
    serde_json::from_str(text.as_ref()).expect("parse ServerMessage")
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

async fn wait_for_room_id(r: &mut Reader) -> RoomId {
    let m = wait_for(r, |m| matches!(m, ServerMessage::RoomState { .. })).await;
    if let ServerMessage::RoomState { state } = m {
        return state.room_id;
    }
    unreachable!()
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

#[allow(dead_code)]
fn _unused_sample() {
    let _: Value = Value::Null;
}
