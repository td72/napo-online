use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use rand::RngExt;
use rand::seq::SliceRandom;
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::db::{Db, GameOutcome, GameRecord};
use crate::game_adapter::{ApplyOutcome, FinishedHandSummary, GameSession};
use crate::now_unix;
use crate::protocol::{
    PlayerId, PlayerView, RoomId, RoomPhase as WireRoomPhase, RoomView, ServerMessage, WireAction,
};

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub player_id: PlayerId,
    pub display_name: String,
}

/// Room actor のタイマー設定。env から組み立てて Hub 経由で各 Room に渡す。
#[derive(Debug, Clone, Copy)]
pub struct RoomTimers {
    pub t2_notify: Duration,
    pub t3_abort: Duration,
    pub t4_end: Duration,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnState {
    Online,
    Suspended,
}

struct RoomMember {
    info: PlayerInfo,
    sender: mpsc::Sender<ServerMessage>,
    conn: ConnState,
    /// Suspended になった時刻。T2/T3/T4 判定の起点。
    suspended_since: Option<Instant>,
    /// PlayerDisconnected を既にブロードキャスト済みか(T2 一度だけ通知用)
    notified_disconnect: bool,
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
    db: Db,
    timers: RoomTimers,
    current_hand_started_at: Option<i64>,
}

impl Room {
    pub fn new(
        id: RoomId,
        host: PlayerId,
        rx: mpsc::Receiver<RoomCommand>,
        max_players: usize,
        hands_per_match: u32,
        db: Db,
        timers: RoomTimers,
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
            db,
            timers,
            current_hand_started_at: None,
        }
    }

    pub async fn run(mut self) {
        tracing::info!(room = %self.id, host = %self.host, "room actor started");
        let mut tick = interval(Duration::from_secs(1));
        // 初回 tick は即時に発火する仕様 — 一度吸ってから loop に入る
        tick.tick().await;
        loop {
            tokio::select! {
                cmd = self.rx.recv() => {
                    match cmd {
                        Some(c) => self.handle(c).await,
                        None => break,
                    }
                }
                _ = tick.tick() => {
                    self.on_tick().await;
                }
            }
            if self.should_terminate() {
                break;
            }
        }
        tracing::info!(room = %self.id, "room actor exiting");
    }

    fn should_terminate(&self) -> bool {
        if self.members.is_empty() {
            return true;
        }
        let all_suspended = self.members.values().all(|m| m.conn == ConnState::Suspended);
        if !all_suspended {
            return false;
        }
        // 全員 Suspended かつ Ended → 即終了
        if matches!(self.phase, LocalPhase::Ended) {
            return true;
        }
        // 全員 Suspended で最も古い切断から T4 経過 → 自動終了
        let now = Instant::now();
        self.members
            .values()
            .filter_map(|m| m.suspended_since)
            .min()
            .map(|since| now.duration_since(since) >= self.timers.t4_end)
            .unwrap_or(false)
    }

    async fn handle(&mut self, cmd: RoomCommand) {
        match cmd {
            RoomCommand::Join { info, reply } => self.on_join(info, reply).await,
            RoomCommand::Leave { player } => self.on_leave(player).await,
            RoomCommand::Disconnect { player } => self.on_disconnect(player).await,
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
        // 既存 seated player の再接続
        if let Some(existing) = self.members.get_mut(&info.player_id) {
            existing.sender = reply.clone();
            existing.conn = ConnState::Online;
            existing.suspended_since = None;
            let was_notified = existing.notified_disconnect;
            existing.notified_disconnect = false;
            existing.info.display_name = info.display_name.clone();

            tracing::info!(room = %self.id, player = %info.player_id, "player reconnected");

            // 再接続側に最新の状態を送る
            if let Some(session) = self.session.as_ref()
                && let Some(seat) = session.seat_of(&info.player_id)
            {
                let obs = session
                    .observations_for_all()
                    .remove(&info.player_id)
                    .unwrap();
                let _ = reply
                    .send(ServerMessage::Observation { observation: obs })
                    .await;
                if was_notified {
                    self.broadcast(ServerMessage::PlayerReconnected { seat })
                        .await;
                }
            }
            self.broadcast_room_state().await;
            return;
        }

        // 新規 join は Lobby のみ
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
                conn: ConnState::Online,
                suspended_since: None,
                notified_disconnect: false,
            },
        );
        tracing::info!(room = %self.id, player = %pid, "player joined");
        self.broadcast_room_state().await;
    }

    async fn on_leave(&mut self, player: PlayerId) {
        // 明示的な退室は常に削除
        if self.members.remove(&player).is_some() {
            tracing::info!(room = %self.id, %player, "player left");
            if !self.members.is_empty() {
                self.broadcast_room_state().await;
            }
        }
    }

    async fn on_disconnect(&mut self, player: PlayerId) {
        // 進行中(Playing/BetweenHands)で seated なら席を残して Suspended、
        // それ以外(Lobby/Ended)は普通に退室扱い
        let in_session = matches!(
            self.phase,
            LocalPhase::Playing | LocalPhase::BetweenHands
        );
        let is_seated = self
            .session
            .as_ref()
            .map(|s| s.seat_of(&player).is_some())
            .unwrap_or(false);

        if in_session && is_seated {
            if let Some(m) = self.members.get_mut(&player) {
                m.conn = ConnState::Suspended;
                m.suspended_since = Some(Instant::now());
                m.notified_disconnect = false;
                tracing::info!(room = %self.id, %player, "player suspended (disconnect)");
                self.broadcast_room_state().await;
            }
        } else if self.members.remove(&player).is_some() {
            tracing::info!(room = %self.id, %player, "player disconnected (removed, not in session)");
            if !self.members.is_empty() {
                self.broadcast_room_state().await;
            }
        }
    }

    async fn on_tick(&mut self) {
        // T2: 切断後 t2_notify が経過したら一度だけ PlayerDisconnected をブロードキャスト
        let now = Instant::now();
        let t2 = self.timers.t2_notify;
        let mut to_notify: Vec<usize> = Vec::new();
        for m in self.members.values_mut() {
            if m.conn == ConnState::Suspended
                && !m.notified_disconnect
                && let Some(since) = m.suspended_since
                && now.duration_since(since) >= t2
            {
                let seat = self
                    .session
                    .as_ref()
                    .and_then(|s| s.seat_of(&m.info.player_id));
                if let Some(s) = seat {
                    to_notify.push(s);
                }
                m.notified_disconnect = true;
            }
        }
        for seat in to_notify {
            let since_ms = self
                .members
                .values()
                .find(|m| {
                    self.session
                        .as_ref()
                        .and_then(|s| s.seat_of(&m.info.player_id))
                        == Some(seat)
                })
                .and_then(|m| m.suspended_since)
                .map(|i| now.duration_since(i).as_millis() as u64)
                .unwrap_or(0);
            self.broadcast(ServerMessage::PlayerDisconnected { seat, since_ms })
                .await;
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
        self.current_hand_started_at = Some(now_unix());
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
                self.current_hand_started_at = Some(now_unix());
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
        if !self.any_seated_disconnect_exceeded(self.timers.t3_abort) {
            self.send_to(
                &by,
                error(
                    "too_early",
                    "AbortHand は T3 経過(切断したプレイヤーがいる場合)後のみ可能",
                ),
            )
            .await;
            return;
        }
        // napo-core は途中の hand を「捨てる」操作を持たない。
        // V1 では「セット全体を終了」として扱う(EndMatch と同等。次の Match を別部屋で開始する)。
        self.end_match("aborted_by_host").await;
    }

    async fn on_end_match(&mut self, by: PlayerId) {
        if by != self.host {
            self.send_to(&by, error("not_host", "only host can end match"))
                .await;
            return;
        }
        if !self.any_seated_disconnect_exceeded(self.timers.t4_end) {
            self.send_to(
                &by,
                error(
                    "too_early",
                    "EndMatch は T4 経過(切断したプレイヤーがいる場合)後のみ可能",
                ),
            )
            .await;
            return;
        }
        self.end_match("ended_by_host").await;
    }

    fn any_seated_disconnect_exceeded(&self, threshold: Duration) -> bool {
        let now = Instant::now();
        self.members.values().any(|m| {
            m.conn == ConnState::Suspended
                && m.suspended_since
                    .map(|i| now.duration_since(i) >= threshold)
                    .unwrap_or(false)
        })
    }

    async fn end_match(&mut self, reason: &str) {
        let final_scores = self
            .session
            .as_ref()
            .map(|s| s.cumulative_scores())
            .unwrap_or_default();
        tracing::info!(room = %self.id, %reason, ?final_scores, "match ended early");
        self.phase = LocalPhase::Ended;
        self.broadcast(ServerMessage::MatchEnded { final_scores }).await;
        self.broadcast_room_state().await;
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
            // DB 永続化: 局のサマリを games / game_players に書く
            if let Some(summary) = self.session.as_ref().and_then(|s| s.finished_hand_summary()) {
                self.persist_finished_hand(&summary).await;
            }

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

    async fn persist_finished_hand(&self, summary: &FinishedHandSummary) {
        let game_id = uuid::Uuid::new_v4().to_string();
        let started_at = self.current_hand_started_at.unwrap_or_else(now_unix);
        let ended_at = now_unix();
        let players: Vec<(String, usize)> = summary
            .seat_scores
            .iter()
            .enumerate()
            .map(|(seat, (email, _))| (email.clone(), seat))
            .collect();
        let record = GameRecord {
            id: game_id.clone(),
            room_id: self.id.clone(),
            started_at,
            napoleon: Some(summary.declarer_player.clone()),
            adjutant: summary.adjutant_player.clone(),
            bid_value: Some(summary.bid_value as i32),
            bid_suit: Some(summary.bid_suit.clone()),
        };
        if let Err(e) = self.db.record_game_start(record, players).await {
            tracing::warn!(error = %e, "record_game_start failed");
            return;
        }
        let outcome = GameOutcome {
            ended_at,
            napoleon_won: Some(summary.napoleon_won),
            seat_scores: summary.seat_scores.clone(),
        };
        if let Err(e) = self.db.record_game_end(&game_id, outcome).await {
            tracing::warn!(error = %e, "record_game_end failed");
        } else {
            tracing::info!(room = %self.id, %game_id, "hand persisted");
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
                online: m.conn == ConnState::Online,
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
            // Suspended でもチャネルが生きていれば送る(再接続時にバッファ取得は今は実装せず)
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
