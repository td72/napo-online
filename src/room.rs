use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use napo_core::Match;
use rand::RngExt;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::db::{Db, GameOutcome, GameRecord};
use crate::game_adapter::{ApplyOutcome, FinishedHandSummary, GameSession};
use crate::now_unix;
use crate::protocol::{
    PlayerId, PlayerView, PublicEvent, RoomId, RoomPhase as WireRoomPhase, RoomView, ServerMessage,
    WireAction,
};

/// 永続化用のスナップショット。`napo_core::Match` を直接埋め込む(serde feature 必須)。
/// chat は別テーブル(chat_messages)で管理するため snapshot には含めない。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub schema: u32,
    pub room_id: RoomId,
    pub host: PlayerId,
    pub hands_per_match: u32,
    pub max_players: usize,
    pub members: Vec<MemberSnapshot>,
    pub phase: PhaseSnapshot,
    pub session: Option<SessionSnapshot>,
    pub next_ready: Vec<usize>,
    pub current_hand_started_at: Option<i64>,
}

pub const ROOM_SNAPSHOT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberSnapshot {
    pub player_id: PlayerId,
    pub display_name: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub is_com: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PhaseSnapshot {
    Lobby,
    Playing,
    BetweenHands,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session: Match,
    pub seat_to_player: Vec<PlayerId>,
    pub seat_display_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub player_id: PlayerId,
    pub display_name: String,
    pub avatar: String,
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
    /// host が COM を追加。
    AddCom {
        by: PlayerId,
        display_name: String,
    },
    Chat {
        player: PlayerId,
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnState {
    Online,
    Suspended,
}

struct RoomMember {
    info: PlayerInfo,
    /// 生きている WS チャンネル。スナップショット復元直後は None(全員 Suspended で再接続待ち)。
    /// COM の場合は常に None。
    sender: Option<mpsc::Sender<ServerMessage>>,
    conn: ConnState,
    /// Suspended になった時刻。T2/T3/T4 判定の起点。
    suspended_since: Option<Instant>,
    /// PlayerDisconnected を既にブロードキャスト済みか(T2 一度だけ通知用)
    notified_disconnect: bool,
    /// COM(bot)プレイヤーかどうか。
    is_com: bool,
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

const CHAT_TEXT_MAX_CHARS: usize = 500;
/// 入室時に backlog として送る最新メッセージ数。これより古いものは scrollback で取得。
const CHAT_BACKLOG_INITIAL: usize = 100;

fn chrono_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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

    /// スナップショットから復元する。全 member は sender=None / Suspended で
    /// 入っており、JoinRoom (= 再接続)で sender がセットされて Online に戻る。
    pub fn from_snapshot(
        snapshot: RoomSnapshot,
        rx: mpsc::Receiver<RoomCommand>,
        db: Db,
        timers: RoomTimers,
    ) -> Self {
        let session = snapshot.session.map(|s| {
            GameSession::from_parts(
                s.session,
                s.seat_to_player,
                s.seat_display_names,
                snapshot.hands_per_match,
            )
        });
        let now = Instant::now();
        let mut members: HashMap<PlayerId, RoomMember> = HashMap::new();
        for m in snapshot.members {
            members.insert(
                m.player_id.clone(),
                RoomMember {
                    info: PlayerInfo {
                        player_id: m.player_id,
                        display_name: m.display_name,
                        avatar: m.avatar,
                    },
                    sender: None,
                    conn: ConnState::Suspended,
                    suspended_since: Some(now),
                    notified_disconnect: false,
                    is_com: m.is_com,
                },
            );
        }
        let phase = match snapshot.phase {
            PhaseSnapshot::Lobby => LocalPhase::Lobby,
            PhaseSnapshot::Playing => LocalPhase::Playing,
            PhaseSnapshot::BetweenHands => LocalPhase::BetweenHands,
            PhaseSnapshot::Ended => LocalPhase::Ended,
        };
        Self {
            id: snapshot.room_id,
            host: snapshot.host,
            members,
            rx,
            max_players: snapshot.max_players,
            phase,
            session,
            next_ready: snapshot.next_ready.into_iter().collect(),
            hands_per_match: snapshot.hands_per_match,
            db,
            timers,
            current_hand_started_at: snapshot.current_hand_started_at,
        }
    }

    fn snapshot(&self) -> RoomSnapshot {
        let session = self.session.as_ref().map(|s| SessionSnapshot {
            session: s.match_session().clone(),
            seat_to_player: s.seat_to_player().to_vec(),
            seat_display_names: s.seat_display_names().to_vec(),
        });
        RoomSnapshot {
            schema: ROOM_SNAPSHOT_SCHEMA,
            room_id: self.id.clone(),
            host: self.host.clone(),
            hands_per_match: self.hands_per_match,
            max_players: self.max_players,
            members: self
                .members
                .values()
                .map(|m| MemberSnapshot {
                    player_id: m.info.player_id.clone(),
                    display_name: m.info.display_name.clone(),
                    avatar: m.info.avatar.clone(),
                    is_com: m.is_com,
                })
                .collect(),
            phase: match self.phase {
                LocalPhase::Lobby => PhaseSnapshot::Lobby,
                LocalPhase::Playing => PhaseSnapshot::Playing,
                LocalPhase::BetweenHands => PhaseSnapshot::BetweenHands,
                LocalPhase::Ended => PhaseSnapshot::Ended,
            },
            session,
            next_ready: {
                let mut v: Vec<usize> = self.next_ready.iter().copied().collect();
                v.sort_unstable();
                v
            },
            current_hand_started_at: self.current_hand_started_at,
        }
    }

    async fn persist(&self) {
        let snap = self.snapshot();
        let json = match serde_json::to_string(&snap) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(room = %self.id, error = %e, "snapshot serialize failed");
                return;
            }
        };
        if let Err(e) = self
            .db
            .save_room_snapshot(&self.id, &json, now_unix())
            .await
        {
            tracing::warn!(room = %self.id, error = %e, "save_room_snapshot failed");
        }
    }

    async fn unpersist(&self) {
        if let Err(e) = self.db.delete_room_snapshot(&self.id).await {
            tracing::warn!(room = %self.id, error = %e, "delete_room_snapshot failed");
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
        // クリーンアップ: 終了状態 / 全員退室で snapshot を消す
        self.unpersist().await;
        tracing::info!(room = %self.id, "room actor exiting");
    }

    fn should_terminate(&self) -> bool {
        if self.members.is_empty() {
            return true;
        }
        let all_suspended = self
            .members
            .values()
            .all(|m| m.conn == ConnState::Suspended);
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
            RoomCommand::AddCom { by, display_name } => self.on_add_com(by, display_name).await,
            RoomCommand::Chat { player, text } => self.on_chat(player, text).await,
        }
    }

    async fn on_chat(&mut self, player: PlayerId, text: String) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let truncated: String = trimmed.chars().take(CHAT_TEXT_MAX_CHARS).collect();
        let Some(m) = self.members.get(&player) else {
            return;
        };
        let sender_id = m.info.player_id.clone();
        let sender_name = m.info.display_name.clone();
        let sender_avatar = m.info.avatar.clone();
        let ts_ms = chrono_millis();
        let seq = match self
            .db
            .insert_chat(
                &self.id,
                &sender_id,
                &sender_name,
                &sender_avatar,
                &truncated,
                ts_ms,
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(room = %self.id, error = %e, "insert_chat failed");
                // 永続化失敗時は配信もスキップ(client 間で seq の整合が崩れるため)。
                return;
            }
        };
        let entry = crate::protocol::ChatEntry {
            seq,
            sender_id,
            sender_name,
            sender_avatar,
            text: truncated,
            ts_ms,
        };
        self.broadcast(ServerMessage::Chat { entry }).await;
    }

    async fn on_join(&mut self, info: PlayerInfo, reply: mpsc::Sender<ServerMessage>) {
        // 既存 player の再接続(seated でも、Lobby で席無しでも、snapshot 復元直後でも)
        if let Some(existing) = self.members.get_mut(&info.player_id) {
            existing.sender = Some(reply.clone());
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
            // chat 履歴を再送
            self.send_chat_backlog(&reply).await;
            self.broadcast_room_state().await;
            self.persist().await;
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
        let reply_clone = reply.clone();
        self.members.insert(
            pid.clone(),
            RoomMember {
                info,
                sender: Some(reply),
                conn: ConnState::Online,
                suspended_since: None,
                notified_disconnect: false,
                is_com: false,
            },
        );
        tracing::info!(room = %self.id, player = %pid, "player joined");
        self.send_chat_backlog(&reply_clone).await;
        self.broadcast_room_state().await;
        self.persist().await;
    }

    /// 指定 sender へ最新 N 件の chat 履歴を ChatBacklog で送る。
    /// それより古いものは client が scrollback で別途取得する。
    async fn send_chat_backlog(&self, reply: &mpsc::Sender<ServerMessage>) {
        match self
            .db
            .list_chat(&self.id, None, CHAT_BACKLOG_INITIAL)
            .await
        {
            Ok(entries) if !entries.is_empty() => {
                let _ = reply.send(ServerMessage::ChatBacklog { entries }).await;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(room = %self.id, error = %e, "list_chat failed");
            }
        }
    }

    async fn on_add_com(&mut self, by: PlayerId, display_name: String) {
        if by != self.host {
            self.send_to(&by, error("not_host", "only host can add COM"))
                .await;
            return;
        }
        if self.phase != LocalPhase::Lobby {
            self.send_to(&by, error("wrong_phase", "COM can only be added in lobby"))
                .await;
            return;
        }
        if self.members.len() >= self.max_players {
            self.send_to(&by, error("room_full", "no slot for COM"))
                .await;
            return;
        }
        // unique COM player_id を発行(com1, com2, ... 衝突しない番号)
        let mut idx = 1usize;
        let pid = loop {
            let candidate = format!("com{idx}@local.napo.bot");
            if !self.members.contains_key(&candidate) {
                break candidate;
            }
            idx += 1;
        };
        let name = if display_name.trim().is_empty() {
            format!("COM-{idx}")
        } else {
            display_name.trim().to_string()
        };
        self.members.insert(
            pid.clone(),
            RoomMember {
                info: PlayerInfo {
                    player_id: pid.clone(),
                    display_name: name,
                    avatar: "🤖".to_string(),
                },
                sender: None,
                conn: ConnState::Online,
                suspended_since: None,
                notified_disconnect: false,
                is_com: true,
            },
        );
        tracing::info!(room = %self.id, com = %pid, "COM added");
        self.broadcast_room_state().await;
        self.persist().await;
    }

    async fn on_leave(&mut self, player: PlayerId) {
        // 明示的な退室は常に削除
        if self.members.remove(&player).is_some() {
            tracing::info!(room = %self.id, %player, "player left");
            if !self.members.is_empty() {
                self.broadcast_room_state().await;
                self.persist().await;
            }
        }
    }

    async fn on_disconnect(&mut self, player: PlayerId) {
        // 進行中(Playing/BetweenHands)で seated なら席を残して Suspended、
        // それ以外(Lobby/Ended)は普通に退室扱い
        let in_session = matches!(self.phase, LocalPhase::Playing | LocalPhase::BetweenHands);
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
        self.persist().await;
        self.drive_coms().await;
    }

    async fn on_submit_action(&mut self, player: PlayerId, action: WireAction) {
        if self.phase != LocalPhase::Playing {
            self.send_to(&player, error("wrong_phase", "not in playing phase"))
                .await;
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match session.apply(&player, action) {
            Ok(outcome) => {
                self.dispatch_outcome(outcome).await;
                self.persist().await;
                self.drive_coms().await;
            }
            Err(e) => {
                self.send_to(&player, error("invalid_action", &e.to_string()))
                    .await;
            }
        }
    }

    /// 次の to_act が COM プレイヤーである限り、ディレイ付きで自動的に手を打つ。
    /// 1 回 step を踏むごとに dispatch_outcome を流すので、各 COM の手が
    /// observation 経由で全クライアントに届く。
    async fn drive_coms(&mut self) {
        // 安全のため最大ステップ数で打ち切り(無限ループ予防)
        for step in 0..200 {
            // 次に行動すべき seat を取得
            let Some(session) = self.session.as_ref() else {
                return;
            };
            let Some(seat) = session.current_player() else {
                tracing::debug!(room = %self.id, step, "drive_coms: no current_player, exit");
                return;
            };
            let Some(pid) = session.seat_to_player().get(seat).cloned() else {
                return;
            };
            // COM じゃなければ抜ける(人間に手番を返す)
            let Some(member) = self.members.get(&pid) else {
                return;
            };
            if !member.is_com {
                tracing::debug!(room = %self.id, step, %pid, "drive_coms: human turn, exit");
                return;
            }
            tracing::debug!(room = %self.id, step, com = %pid, "drive_coms: COM turn, sleeping");
            // 視覚的に分かるように軽いディレイ。NAPO_COM_DELAY_MS で上書き可。
            let delay_ms: u64 = std::env::var("NAPO_COM_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(500);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            // 現在 phase に対する legal action の先頭を選ぶ
            let Some(wire) = self.session.as_ref().and_then(|s| s.pick_com_action()) else {
                tracing::warn!(room = %self.id, com = %pid, "COM no legal action; bail");
                return;
            };
            tracing::debug!(room = %self.id, step, com = %pid, ?wire, "drive_coms: apply");
            // apply
            let session_mut = self.session.as_mut().unwrap();
            match session_mut.apply(&pid, wire) {
                Ok(outcome) => {
                    self.dispatch_outcome(outcome).await;
                    self.persist().await;
                }
                Err(e) => {
                    tracing::error!(room = %self.id, com = %pid, error = %e, "COM apply failed");
                    return;
                }
            }
        }
        tracing::warn!(room = %self.id, "drive_coms hit step cap");
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
        // COM 席は自動 ready 扱い。1 人 + COM のみ卓でも human だけで進行可能。
        if let Some(session) = self.session.as_ref() {
            for (s, pid) in session.seat_to_player().iter().enumerate() {
                if self.members.get(pid).is_some_and(|m| m.is_com) {
                    self.next_ready.insert(s);
                }
            }
        }
        let mut ready: Vec<usize> = self.next_ready.iter().copied().collect();
        ready.sort_unstable();
        self.broadcast(ServerMessage::NextHandReady { ready_seats: ready })
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
                // BetweenHands → Playing の遷移を client にも伝える(これが無いと
                // EndOverlay が消えず、新局の BidOverlay が裏に隠れる)
                self.broadcast_room_state().await;
                self.persist().await;
                self.drive_coms().await;
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
        if self.any_human_in_grace_period(self.timers.t3_abort) {
            self.send_to(
                &by,
                error(
                    "too_early",
                    "切断中プレイヤーの再接続待機(T3)が経過するまで AbortHand 不可",
                ),
            )
            .await;
            return;
        }
        let Some(session) = self.session.as_mut() else {
            self.send_to(&by, error("wrong_phase", "no session to abort"))
                .await;
            return;
        };
        if let Err(e) = session.abandon_current_hand() {
            self.send_to(&by, error("invalid_state", &e.to_string()))
                .await;
            return;
        }
        let hand_scores: Vec<i32> = session
            .match_session()
            .current_hand()
            .scores()
            .unwrap()
            .to_vec();
        let cumulative_before = session.cumulative_scores();
        let cumulative: Vec<i32> = cumulative_before
            .iter()
            .zip(hand_scores.iter())
            .map(|(a, b)| a + b)
            .collect();
        let next_index = session.match_session().hand_count() as u32 + 1;
        let match_finished = next_index >= self.hands_per_match;
        tracing::info!(room = %self.id, ?hand_scores, "hand aborted by host");

        self.broadcast(ServerMessage::Event {
            event: PublicEvent::HandAborted,
        })
        .await;
        self.broadcast(ServerMessage::HandEnded {
            scores: hand_scores,
            cumulative: cumulative.clone(),
        })
        .await;

        if match_finished {
            self.phase = LocalPhase::Ended;
            self.broadcast(ServerMessage::MatchEnded {
                final_scores: cumulative,
            })
            .await;
            self.broadcast_room_state().await;
            self.unpersist().await;
        } else {
            self.phase = LocalPhase::BetweenHands;
            self.next_ready.clear();
            self.broadcast_room_state().await;
            self.persist().await;
        }
    }

    async fn on_end_match(&mut self, by: PlayerId) {
        if by != self.host {
            self.send_to(&by, error("not_host", "only host can end match"))
                .await;
            return;
        }
        if self.any_human_in_grace_period(self.timers.t4_end) {
            self.send_to(
                &by,
                error(
                    "too_early",
                    "切断中プレイヤーの再接続待機(T4)が経過するまで EndMatch 不可",
                ),
            )
            .await;
            return;
        }
        self.end_match("ended_by_host").await;
    }

    /// 「人間が切断 grace period 中(切断後 threshold 未満)」かどうか。
    /// COM は常に Online 扱いなので除外。Host による AbortHand/EndMatch の gate 用。
    /// 全員 online(COM のみ含む)の場合は false → 操作通過。
    fn any_human_in_grace_period(&self, threshold: Duration) -> bool {
        let now = Instant::now();
        self.members.values().any(|m| {
            !m.is_com
                && m.conn == ConnState::Suspended
                && m.suspended_since
                    .map(|i| now.duration_since(i) < threshold)
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
        self.broadcast(ServerMessage::MatchEnded { final_scores })
            .await;
        self.broadcast_room_state().await;
        self.unpersist().await;
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
            if let Some(summary) = self
                .session
                .as_ref()
                .and_then(|s| s.finished_hand_summary())
            {
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
                    self.unpersist().await;
                    return;
                }
            }
            self.phase = LocalPhase::BetweenHands;
            self.next_ready.clear();
            self.broadcast_room_state().await;
            self.persist().await;
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
                avatar: m.info.avatar.clone(),
                seat: self
                    .session
                    .as_ref()
                    .and_then(|s| s.seat_of(&m.info.player_id)),
                online: m.conn == ConnState::Online || m.is_com,
                is_com: m.is_com,
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
            if let Some(s) = &member.sender {
                let _ = s.send(msg.clone()).await;
            }
        }
    }

    async fn send_to(&self, player: &PlayerId, msg: ServerMessage) {
        if let Some(m) = self.members.get(player)
            && let Some(s) = &m.sender
        {
            let _ = s.send(msg).await;
        }
    }
}

fn error(code: &str, message: &str) -> ServerMessage {
    ServerMessage::Error {
        code: code.into(),
        message: message.into(),
    }
}
