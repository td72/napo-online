#![allow(dead_code)] // wire 型はコンシューマ(ws.rs / room.rs)が揃うまで未参照

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;

pub type PlayerId = String;
pub type RoomId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello { display_name: String },
    CreateRoom,
    JoinRoom { room_id: RoomId },
    LeaveRoom,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        player_id: PlayerId,
        server_version: String,
    },
    RoomState {
        state: RoomView,
    },
    Error {
        code: String,
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomView {
    pub room_id: RoomId,
    pub host: PlayerId,
    pub players: Vec<PlayerView>,
    pub state: RoomPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    pub player_id: PlayerId,
    pub display_name: String,
    pub seat: Option<usize>,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
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
}
