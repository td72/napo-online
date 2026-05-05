use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::protocol::{PlayerId, PlayerView, RoomId, RoomPhase, RoomView, ServerMessage};

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub player_id: PlayerId,
    pub display_name: String,
}

pub enum RoomCommand {
    Join {
        info: PlayerInfo,
        reply: mpsc::Sender<ServerMessage>,
    },
    Leave {
        player: PlayerId,
    },
    Disconnect {
        player: PlayerId,
    },
}

struct RoomMember {
    info: PlayerInfo,
    sender: mpsc::Sender<ServerMessage>,
}

pub struct Room {
    id: RoomId,
    host: PlayerId,
    members: HashMap<PlayerId, RoomMember>,
    rx: mpsc::Receiver<RoomCommand>,
    max_players: usize,
}

impl Room {
    pub fn new(
        id: RoomId,
        host: PlayerId,
        rx: mpsc::Receiver<RoomCommand>,
        max_players: usize,
    ) -> Self {
        Self {
            id,
            host,
            members: HashMap::new(),
            rx,
            max_players,
        }
    }

    pub async fn run(mut self) {
        tracing::info!(room = %self.id, host = %self.host, "room actor started");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                RoomCommand::Join { info, reply } => {
                    if self.members.len() >= self.max_players {
                        let _ = reply
                            .send(ServerMessage::Error {
                                code: "room_full".into(),
                                message: format!("room {} is full", self.id),
                            })
                            .await;
                        continue;
                    }
                    let pid = info.player_id.clone();
                    self.members.insert(
                        pid.clone(),
                        RoomMember {
                            info,
                            sender: reply,
                        },
                    );
                    tracing::info!(room = %self.id, player = %pid, "player joined");
                    self.broadcast_state().await;
                }
                RoomCommand::Leave { player } | RoomCommand::Disconnect { player } => {
                    if self.members.remove(&player).is_some() {
                        tracing::info!(room = %self.id, %player, "player left");
                        if self.members.is_empty() {
                            break;
                        }
                        self.broadcast_state().await;
                    }
                }
            }
        }
        tracing::info!(room = %self.id, "room actor exiting");
    }

    fn build_view(&self) -> RoomView {
        let mut players: Vec<PlayerView> = self
            .members
            .values()
            .map(|m| PlayerView {
                player_id: m.info.player_id.clone(),
                display_name: m.info.display_name.clone(),
                seat: None,
                online: true,
            })
            .collect();
        players.sort_by(|a, b| a.player_id.cmp(&b.player_id));
        RoomView {
            room_id: self.id.clone(),
            host: self.host.clone(),
            players,
            state: RoomPhase::Lobby,
        }
    }

    async fn broadcast_state(&self) {
        let msg = ServerMessage::RoomState {
            state: self.build_view(),
        };
        for member in self.members.values() {
            let _ = member.sender.send(msg.clone()).await;
        }
    }
}
