use std::collections::{HashMap, HashSet};

use rand::RngExt;
use rand::seq::SliceRandom;
use tokio::sync::mpsc;

use crate::game_adapter::{ApplyOutcome, GameSession};
use crate::protocol::{
    PlayerId, PlayerView, RoomId, RoomPhase as WireRoomPhase, RoomView, ServerMessage, WireAction,
};

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
    StartMatch {
        by: PlayerId,
    },
    AbortHand {
        by: PlayerId,
    },
    EndMatch {
        by: PlayerId,
    },
    NextHand {
        player: PlayerId,
    },
    SubmitAction {
        player: PlayerId,
        action: WireAction,
    },
}

struct RoomMember {
    info: PlayerInfo,
    sender: mpsc::Sender<ServerMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalPhase {
    Lobby,
    Playing,
    BetweenHands,
    Ended,
}

pub struct Room {
    id: RoomId,
    host: PlayerId,
    members: HashMap<PlayerId, RoomMember>,
    rx: mpsc::Receiver<RoomCommand>,
    max_players: usize,
    phase: LocalPhase,
    session: Option<GameSession>,
    next_ready: HashSet<usize>,
    hands_per_match: u32,
}

impl Room {
    pub fn new(
        id: RoomId,
        host: PlayerId,
        rx: mpsc::Receiver<RoomCommand>,
        max_players: usize,
        hands_per_match: u32,
    ) -> Self {
        Self {
            id,
            host,
            members: HashMap::new(),
            rx,
            max_players,
            phase: LocalPhase::Lobby,
            session: None,
            next_ready: HashSet::new(),
            hands_per_match,
        }
    }

    pub async fn run(mut self) {
        tracing::info!(room = %self.id, host = %self.host, "room actor started");
        while let Some(cmd) = self.rx.recv().await {
            self.handle(cmd).await;
            // 全員退室で停止
            if self.members.is_empty() && matches!(self.phase, LocalPhase::Lobby | LocalPhase::Ended) {
                break;
            }
        }
        tracing::info!(room = %self.id, "room actor exiting");
    }

    async fn handle(&mut self, cmd: RoomCommand) {
        match cmd {
            RoomCommand::Join { info, reply } => self.on_join(info, reply).await,
            RoomCommand::Leave { player } | RoomCommand::Disconnect { player } => {
                self.on_leave(player).await
            }
            RoomCommand::StartMatch { by } => self.on_start_match(by).await,
            RoomCommand::SubmitAction { player, action } => {
                self.on_submit_action(player, action).await
            }
            RoomCommand::NextHand { player } => self.on_next_hand(player).await,
            RoomCommand::AbortHand { by } => self.on_abort_hand(by).await,
            RoomCommand::EndMatch { by } => self.on_end_match(by).await,
        }
    }

    async fn on_join(&mut self, info: PlayerInfo, reply: mpsc::Sender<ServerMessage>) {
        if self.phase != LocalPhase::Lobby {
            let _ = reply
                .send(error("match_in_progress", "this room is mid-match"))
                .await;
            return;
        }
        if self.members.len() >= self.max_players {
            let _ = reply
                .send(error("room_full", &format!("room {} is full", self.id)))
                .await;
            return;
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
        self.broadcast_room_state().await;
    }

    async fn on_leave(&mut self, player: PlayerId) {
        if self.members.remove(&player).is_some() {
            tracing::info!(room = %self.id, %player, "player left");
            if !self.members.is_empty() {
                self.broadcast_room_state().await;
            }
        }
    }

    async fn on_start_match(&mut self, by: PlayerId) {
        if by != self.host {
            self.send_to(&by, error("not_host", "only host can start the match"))
                .await;
            return;
        }
        if self.phase != LocalPhase::Lobby {
            self.send_to(&by, error("wrong_phase", "match already started"))
                .await;
            return;
        }
        if self.members.len() != self.max_players {
            self.send_to(
                &by,
                error(
                    "not_enough_players",
                    &format!(
                        "need {} players, have {}",
                        self.max_players,
                        self.members.len()
                    ),
                ),
            )
            .await;
            return;
        }

        // ThreadRng は !Send なので、await を跨がないようブロックに閉じ込めて drop させる
        let (seated, seed) = {
            let mut seated: Vec<(PlayerId, String)> = self
                .members
                .values()
                .map(|m| (m.info.player_id.clone(), m.info.display_name.clone()))
                .collect();
            let mut rng = rand::rng();
            seated.shuffle(&mut rng);
            let seed: u64 = rng.random();
            (seated, seed)
        };
        let session = GameSession::new(seated, seed, self.hands_per_match);
        let assignments = session.seat_assignments();
        let observations = session.observations_for_all();

        self.session = Some(session);
        self.phase = LocalPhase::Playing;
        tracing::info!(room = %self.id, ?assignments, "match started");

        let match_seed = self.session.as_ref().unwrap().base_seed();
        self.broadcast(ServerMessage::MatchStarted {
            hand_count: self.hands_per_match,
            seats: assignments,
            match_seed,
        })
        .await;
        for (pid, obs) in observations {
            self.send_to(&pid, ServerMessage::Observation { observation: obs })
                .await;
        }
        self.broadcast_room_state().await;
    }

    async fn on_submit_action(&mut self, player: PlayerId, action: WireAction) {
        if self.phase != LocalPhase::Playing {
            self.send_to(&player, error("wrong_phase", "not in playing phase"))
                .await;
            return;
        }
        let Some(session) = self.session.as_mut() else { return };
        match session.apply(&player, action) {
            Ok(outcome) => self.dispatch_outcome(outcome).await,
            Err(e) => {
                self.send_to(&player, error("invalid_action", &e.to_string()))
                    .await;
            }
        }
    }

    async fn on_next_hand(&mut self, player: PlayerId) {
        if self.phase != LocalPhase::BetweenHands {
            self.send_to(&player, error("wrong_phase", "not between hands"))
                .await;
            return;
        }
        let Some(seat) = self.session.as_ref().and_then(|s| s.seat_of(&player)) else {
            return;
        };
        self.next_ready.insert(seat);
        let mut ready: Vec<usize> = self.next_ready.iter().copied().collect();
        ready.sort_unstable();
        self.broadcast(ServerMessage::NextHandReady {
            ready_seats: ready,
        })
        .await;

        if self.next_ready.len() < self.max_players {
            return;
        }

        let session = self.session.as_mut().unwrap();
        match session.start_next_hand() {
            Ok(out) => {
                self.next_ready.clear();
                self.phase = LocalPhase::Playing;
                self.dispatch_outcome(out).await;
            }
            Err(e) => {
                tracing::error!(room = %self.id, error = %e, "start_next_hand failed");
            }
        }
    }

    async fn on_abort_hand(&mut self, by: PlayerId) {
        if by != self.host {
            self.send_to(&by, error("not_host", "only host can abort"))
                .await;
            return;
        }
        // Phase 5 で T3 経過チェックを入れる
        self.send_to(
            &by,
            error("not_implemented", "AbortHand: Phase 5 で T3 タイマーと共に実装"),
        )
        .await;
    }

    async fn on_end_match(&mut self, by: PlayerId) {
        if by != self.host {
            self.send_to(&by, error("not_host", "only host can end match"))
                .await;
            return;
        }
        // Phase 5 で T4 経過チェック
        self.send_to(
            &by,
            error("not_implemented", "EndMatch: Phase 5 で T4 タイマーと共に実装"),
        )
        .await;
    }

    async fn dispatch_outcome(&mut self, outcome: ApplyOutcome) {
        for ev in &outcome.events {
            self.broadcast(ServerMessage::Event { event: ev.clone() })
                .await;
        }
        for (pid, obs) in &outcome.observations {
            self.send_to(
                pid,
                ServerMessage::Observation {
                    observation: obs.clone(),
                },
            )
            .await;
        }
        if outcome.hand_finished {
            if let Some(hand_scores) = &outcome.hand_scores {
                // outcome.cumulative_scores は今局を含まないので、合算して送る
                let cumulative: Vec<i32> = outcome
                    .cumulative_scores
                    .iter()
                    .zip(hand_scores)
                    .map(|(a, b)| a + b)
                    .collect();
                self.broadcast(ServerMessage::HandEnded {
                    scores: hand_scores.clone(),
                    cumulative: cumulative.clone(),
                })
                .await;
                if outcome.match_finished {
                    self.phase = LocalPhase::Ended;
                    self.broadcast(ServerMessage::MatchEnded {
                        final_scores: cumulative,
                    })
                    .await;
                    self.broadcast_room_state().await;
                    return;
                }
            }
            self.phase = LocalPhase::BetweenHands;
            self.next_ready.clear();
        }
    }

    fn build_view(&self) -> RoomView {
        let mut players: Vec<PlayerView> = self
            .members
            .values()
            .map(|m| PlayerView {
                player_id: m.info.player_id.clone(),
                display_name: m.info.display_name.clone(),
                seat: self
                    .session
                    .as_ref()
                    .and_then(|s| s.seat_of(&m.info.player_id)),
                online: true,
            })
            .collect();
        players.sort_by(|a, b| a.player_id.cmp(&b.player_id));
        RoomView {
            room_id: self.id.clone(),
            host: self.host.clone(),
            players,
            state: match self.phase {
                LocalPhase::Lobby => WireRoomPhase::Lobby,
                LocalPhase::Playing => WireRoomPhase::Playing,
                LocalPhase::BetweenHands => WireRoomPhase::BetweenHands {
                    hand_index: self
                        .session
                        .as_ref()
                        .map(|s| s.hand_index() + 1)
                        .unwrap_or(0),
                },
                LocalPhase::Ended => WireRoomPhase::Ended,
            },
        }
    }

    async fn broadcast_room_state(&self) {
        let msg = ServerMessage::RoomState {
            state: self.build_view(),
        };
        self.broadcast(msg).await;
    }

    async fn broadcast(&self, msg: ServerMessage) {
        for member in self.members.values() {
            let _ = member.sender.send(msg.clone()).await;
        }
    }

    async fn send_to(&self, player: &PlayerId, msg: ServerMessage) {
        if let Some(m) = self.members.get(player) {
            let _ = m.sender.send(msg).await;
        }
    }
}

fn error(code: &str, message: &str) -> ServerMessage {
    ServerMessage::Error {
        code: code.into(),
        message: message.into(),
    }
}
