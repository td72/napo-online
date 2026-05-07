#![allow(dead_code)] // wire 型はコンシューマ(ws.rs / room.rs)が揃うまで未参照

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const PROTOCOL_VERSION: u8 = 1;

pub type PlayerId = String;
pub type RoomId = String;

/// `cargo test` 時に web/src/types/ へ TS 定義を出力する共通設定。
/// 各 wire 型に `#[ts(export)]` で適用。
const TS_DIR: &str = "web/src/types/";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(export)]
pub enum ClientMessage {
    // ロビー
    Hello { display_name: String },
    CreateRoom,
    JoinRoom { room_id: RoomId },
    LeaveRoom,

    // ホスト操作
    StartMatch,
    AbortHand,
    EndMatch,
    /// COM(bot)プレイヤーを追加。ロビー期間中、host のみ。
    AddCom { display_name: String },

    // 全員操作
    NextHand,
    SubmitAction { action: WireAction },

    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(export)]
pub enum WireAction {
    Bid { tricks: u8, trump: WireTrump },
    Pass,
    DeclareAdjutant { card: String },
    ExchangeWidow { discard: Vec<String> },
    PlayCard { card: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "UPPERCASE")]
#[ts(export)]
pub enum WireTrump {
    S,
    H,
    D,
    C,
    NT,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WireBid {
    pub tricks: u8,
    pub trump: WireTrump,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMessage {
    // 接続/ロビー
    Welcome {
        player_id: PlayerId,
        server_version: String,
        /// 接続者がまだ参加中の room がある場合の room_id。
        /// 切断 → 再接続時、client はこれを見て自動で JoinRoom を送って復帰する。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_room: Option<RoomId>,
    },
    RoomState {
        state: RoomView,
    },
    Error {
        code: String,
        message: String,
    },
    Pong,

    // ゲーム進行(A8: Event と Observation を両方送る)
    Event {
        event: PublicEvent,
    },
    Observation {
        observation: WireObservation,
    },

    // セット管理
    MatchStarted {
        hand_count: u32,
        seats: Vec<SeatAssignment>,
        /// 1局目のシード。`base_seed = seed`、各局は `seed + hand_index` で生成される(`napo_core::Match`)。
        /// クライアントが `napo_core::Match::new(Rules::default(), seed)` で同じ局を再現できる。
        match_seed: u64,
    },
    HandEnded {
        scores: Vec<i32>,
        cumulative: Vec<i32>,
    },
    NextHandReady {
        ready_seats: Vec<usize>,
    },
    MatchEnded {
        final_scores: Vec<i32>,
    },

    // 切断/再接続
    PlayerDisconnected {
        seat: usize,
        since_ms: u64,
    },
    PlayerReconnected {
        seat: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(export)]
pub enum PublicEvent {
    HandStarted {
        hand_index: u32,
        first_bidder: usize,
    },
    Bidded {
        seat: usize,
        bid: WireBid,
    },
    Passed {
        seat: usize,
    },
    NapoleonDecided {
        seat: usize,
        bid: WireBid,
    },
    AdjutantDeclared {
        card: String,
    },
    WidowExchanged {
        revealed_face_cards: Vec<String>,
    },
    CardPlayed {
        seat: usize,
        card: String,
    },
    AdjutantRevealed {
        seat: usize,
    },
    TrickWon {
        seat: usize,
    },
    HandEnded {
        scores: Vec<i32>,
    },
    HandRedealt,
    HandAborted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SeatAssignment {
    pub seat: usize,
    pub player_id: PlayerId,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WirePlayedCard {
    pub seat: usize,
    pub card: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WireCompletedTrick {
    pub plays: Vec<WirePlayedCard>,
    pub winner: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WireBidEntry {
    pub seat: usize,
    /// None = Pass
    pub bid: Option<WireBid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WireCurrentBid {
    pub seat: usize,
    pub bid: WireBid,
}

/// プレイヤー視点のゲーム状態。`napo_core::Observation` を wire 形式に変換したもの。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct WireObservation {
    pub player: usize,
    pub num_players: usize,
    pub hand: Vec<String>,
    pub bid_history: Vec<WireBidEntry>,
    pub current_bid: Option<WireCurrentBid>,
    pub declarer: Option<usize>,
    pub adjutant_card: Option<String>,
    pub adjutant_player: Option<usize>,
    pub completed: Vec<WireCompletedTrick>,
    pub current_trick: Vec<WirePlayedCard>,
    pub trick_index: usize,
    pub to_act: Option<usize>,
    pub visible_widow: Option<Vec<String>>,
    pub revealed_face_discards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoomView {
    pub room_id: RoomId,
    pub host: PlayerId,
    pub players: Vec<PlayerView>,
    pub state: RoomPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlayerView {
    pub player_id: PlayerId,
    pub display_name: String,
    pub seat: Option<usize>,
    pub online: bool,
    /// COM(bot)プレイヤーかどうか。WS は持たず、サーバ側 driver が手を打つ。
    #[serde(default)]
    pub is_com: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(export)]
pub enum RoomPhase {
    Lobby,
    Playing,
    BetweenHands { hand_index: u32 },
    Ended,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hello_serializes_with_payload_object() {
        let m = ClientMessage::Hello {
            display_name: "alice".into(),
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(
            v,
            json!({ "type": "hello", "payload": { "display_name": "alice" } })
        );
    }

    #[test]
    fn join_room_deserializes() {
        let s = r#"{"type":"join_room","payload":{"room_id":"b3K9mP2x"}}"#;
        let m: ClientMessage = serde_json::from_str(s).unwrap();
        match m {
            ClientMessage::JoinRoom { room_id } => assert_eq!(room_id, "b3K9mP2x"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn leave_room_unit_variant_roundtrips() {
        let m = ClientMessage::LeaveRoom;
        let s = serde_json::to_string(&m).unwrap();
        let back: ClientMessage = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, ClientMessage::LeaveRoom));
    }

    #[test]
    fn welcome_roundtrips() {
        let m = ServerMessage::Welcome {
            player_id: "alice@example.com".into(),
            server_version: "0.1.0".into(),
            current_room: None,
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ServerMessage = serde_json::from_str(&s).unwrap();
        match back {
            ServerMessage::Welcome { player_id, .. } => {
                assert_eq!(player_id, "alice@example.com");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn room_state_roundtrips_with_phase_lobby() {
        let m = ServerMessage::RoomState {
            state: RoomView {
                room_id: "ABCD1234".into(),
                host: "host@example.com".into(),
                players: vec![PlayerView {
                    player_id: "alice@example.com".into(),
                    display_name: "alice".into(),
                    seat: None,
                    online: true,
                    is_com: false,
                }],
                state: RoomPhase::Lobby,
            },
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ServerMessage = serde_json::from_str(&s).unwrap();
        match back {
            ServerMessage::RoomState { state } => {
                assert_eq!(state.room_id, "ABCD1234");
                assert_eq!(state.players.len(), 1);
                assert_eq!(state.state, RoomPhase::Lobby);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn room_phase_between_hands_carries_index() {
        let p = RoomPhase::BetweenHands { hand_index: 3 };
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, r#"{"type":"between_hands","payload":{"hand_index":3}}"#);
        let back: RoomPhase = serde_json::from_str(&s).unwrap();
        assert_eq!(back, RoomPhase::BetweenHands { hand_index: 3 });
    }

    #[test]
    fn protocol_version_pinned() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn wire_action_bid_serializes_with_trump_and_tricks() {
        let a = WireAction::Bid {
            tricks: 13,
            trump: WireTrump::S,
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(
            v,
            json!({ "type": "bid", "payload": { "tricks": 13, "trump": "S" } })
        );
    }

    #[test]
    fn wire_action_no_trump_uppercased_as_nt() {
        let a = WireAction::Bid {
            tricks: 14,
            trump: WireTrump::NT,
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains(r#""trump":"NT""#));
    }

    #[test]
    fn wire_action_play_card_roundtrip() {
        let s = r#"{"type":"play_card","payload":{"card":"SA"}}"#;
        let a: WireAction = serde_json::from_str(s).unwrap();
        assert_eq!(a, WireAction::PlayCard { card: "SA".into() });
    }

    #[test]
    fn wire_action_exchange_widow_roundtrip() {
        let a = WireAction::ExchangeWidow {
            discard: vec!["SA".into(), "H10".into()],
        };
        let s = serde_json::to_string(&a).unwrap();
        let back: WireAction = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn submit_action_roundtrip() {
        let m = ClientMessage::SubmitAction {
            action: WireAction::Pass,
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ClientMessage = serde_json::from_str(&s).unwrap();
        match back {
            ClientMessage::SubmitAction { action } => {
                assert_eq!(action, WireAction::Pass);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn public_event_card_played_roundtrip() {
        let e = PublicEvent::CardPlayed {
            seat: 2,
            card: "S10".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: PublicEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn public_event_hand_redealt_unit_variant() {
        let s = serde_json::to_string(&PublicEvent::HandRedealt).unwrap();
        assert!(s.contains(r#""type":"hand_redealt""#));
        let back: PublicEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back, PublicEvent::HandRedealt);
    }

    #[test]
    fn server_message_event_wraps_public_event() {
        let m = ServerMessage::Event {
            event: PublicEvent::Passed { seat: 0 },
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ServerMessage = serde_json::from_str(&s).unwrap();
        match back {
            ServerMessage::Event {
                event: PublicEvent::Passed { seat },
            } => assert_eq!(seat, 0),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn match_started_carries_seat_assignments_and_seed() {
        let m = ServerMessage::MatchStarted {
            hand_count: 10,
            seats: vec![SeatAssignment {
                seat: 0,
                player_id: "alice@example.com".into(),
                display_name: "alice".into(),
            }],
            match_seed: 42,
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: ServerMessage = serde_json::from_str(&s).unwrap();
        match back {
            ServerMessage::MatchStarted {
                hand_count,
                seats,
                match_seed,
            } => {
                assert_eq!(hand_count, 10);
                assert_eq!(seats.len(), 1);
                assert_eq!(seats[0].seat, 0);
                assert_eq!(match_seed, 42);
            }
            _ => panic!("wrong variant"),
        }
    }
}
