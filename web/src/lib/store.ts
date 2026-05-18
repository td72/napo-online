// Zustand store: WS 接続 + サーバから流れてくる全 state を集約。
// component は useStore で必要な slice を取り出す。

import { create } from "zustand";
import { buildWsUrl, openWebSocket } from "./ws";
import type {
  ChatEntry,
  ClientMessage,
  PlayerView,
  PublicEvent,
  RoomView,
  SeatAssignment,
  ServerMessage,
  WireAction,
  WireObservation,
} from "../types";

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "reconnecting";

const PING_INTERVAL_MS = 30_000;
const MAX_EVENT_LOG = 200;
// chat は client 側で上限を設けない(server 側で全件保存しているため、
// scrollback もできる)。古いセッションで膨らみすぎたら reload で backlog(最新 100)
// に戻る。

interface ErrorEntry {
  code: string;
  message: string;
  at: number;
}

interface ScoreReport {
  scores: number[];
  cumulative: number[];
}

interface AppState {
  // ----- Connection / identity -----
  status: ConnectionStatus;
  myEmail: string | null;
  myHandle: string | null;
  myAvatar: string;
  serverVersion: string | null;

  // ----- Room -----
  room: RoomView | null;
  roomId: string | null;

  // ----- Match -----
  matchSeed: number | null;
  hands_per_match: number | null;
  seats: SeatAssignment[] | null;
  mySeat: number | null;
  observation: WireObservation | null;
  /** Next 投票で押した seat 一覧。BetweenHands の進捗表示用。 */
  nextReadySeats: number[];
  /** 切断中の seat と切断時刻(ms)。 */
  disconnectedSeats: Map<number, number>;
  /** 直近 hand のスコア(HandEnded で更新)。 */
  lastScores: ScoreReport | null;
  /** Match 終了時の最終スコア。 */
  finalScores: number[] | null;

  // ----- Log -----
  events: PublicEvent[];
  errors: ErrorEntry[];
  chatLog: ChatEntry[];

  // ----- Actions -----
  connect: (displayName: string, devEmail?: string) => void;
  setHandle: (handle: string) => void;
  send: (msg: ClientMessage) => void;
  submitAction: (action: WireAction) => void;
  disconnect: () => void;
}

let socket: ReturnType<typeof openWebSocket> | null = null;
let pingTimer: ReturnType<typeof setInterval> | null = null;

// 再接続の管理(モジュール内 state)。disconnect() でユーザーが明示切断したら止める。
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectAttempt = 0;
let userInitiatedClose = false;
let lastConnectParams: { displayName: string; devEmail?: string } | null = null;
const RECONNECT_MAX_ATTEMPTS = 10;
const RECONNECT_BACKOFF_MS = [500, 1000, 2000, 4000, 8000, 15000, 30000] as const;
function reconnectDelayMs(attempt: number): number {
  const idx = Math.min(attempt, RECONNECT_BACKOFF_MS.length - 1);
  return RECONNECT_BACKOFF_MS[idx];
}

type SetFn = (partial: Partial<AppState> | ((s: AppState) => Partial<AppState>)) => void;
type GetFn = () => AppState;

/**
 * WS を開いてハンドラを張る。connect() からも自動再接続パスからも呼ばれる。
 * 自動再接続カウンタ(reconnectAttempt)等は呼び出し側で管理する。
 */
function openWsSession(set: SetFn, get: GetFn, displayName: string, devEmail?: string) {
  if (socket) {
    socket.close();
    socket = null;
  }
  set({
    status: reconnectAttempt > 0 ? "reconnecting" : "connecting",
  });
  const url = buildWsUrl(devEmail);
  socket = openWebSocket(url, {
    onOpen: () => {
      reconnectAttempt = 0;
      set({ status: "connected" });
      socket?.send({ type: "hello", payload: { display_name: displayName } });
      if (pingTimer) clearInterval(pingTimer);
      pingTimer = setInterval(() => {
        socket?.send({ type: "ping" });
      }, PING_INTERVAL_MS);
    },
    onClose: () => {
      if (pingTimer) {
        clearInterval(pingTimer);
        pingTimer = null;
      }
      if (userInitiatedClose || !lastConnectParams) {
        set({ status: "disconnected" });
        return;
      }
      if (reconnectAttempt >= RECONNECT_MAX_ATTEMPTS) {
        console.warn("[ws] reconnect: max attempts reached, giving up");
        set({ status: "disconnected" });
        return;
      }
      const delay = reconnectDelayMs(reconnectAttempt);
      reconnectAttempt++;
      console.info(`[ws] reconnect attempt ${reconnectAttempt} in ${delay}ms`);
      set({ status: "reconnecting" });
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        if (lastConnectParams) {
          openWsSession(set, get, lastConnectParams.displayName, lastConnectParams.devEmail);
        }
      }, delay);
    },
    onMessage: (msg) => handleServerMessage(msg, set, get),
    onError: (e) => console.error("ws error", e),
  });
}

export const useStore = create<AppState>((set, get) => ({
  status: "disconnected",
  myEmail: null,
  myHandle: null,
  myAvatar: "",
  serverVersion: null,
  room: null,
  roomId: null,
  matchSeed: null,
  hands_per_match: null,
  seats: null,
  mySeat: null,
  observation: null,
  nextReadySeats: [],
  disconnectedSeats: new Map(),
  lastScores: null,
  finalScores: null,
  events: [],
  errors: [],
  chatLog: [],

  connect: (displayName, devEmail) => {
    // ユーザー操作の connect。自動再接続ステートをリセット。
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    reconnectAttempt = 0;
    userInitiatedClose = false;
    lastConnectParams = { displayName, devEmail };
    openWsSession(set, get, displayName, devEmail);
  },

  setHandle: (handle) => set({ myHandle: handle }),

  send: (msg) => {
    if (!socket) {
      console.warn("not connected, drop:", msg);
      return;
    }
    socket.send(msg);
  },

  submitAction: (action) => {
    socket?.send({ type: "submit_action", payload: { action } });
  },

  disconnect: () => {
    userInitiatedClose = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    reconnectAttempt = 0;
    if (pingTimer) {
      clearInterval(pingTimer);
      pingTimer = null;
    }
    socket?.close();
    socket = null;
    set({ status: "disconnected" });
  },
}));

function handleServerMessage(
  msg: ServerMessage,
  set: (partial: Partial<AppState> | ((s: AppState) => Partial<AppState>)) => void,
  get: () => AppState,
) {
  switch (msg.type) {
    case "welcome":
      set({
        myEmail: msg.payload.player_id,
        serverVersion: msg.payload.server_version,
      });
      // 初期 handle / avatar を /api/me から取得
      void fetch("/api/me")
        .then((r) => (r.ok ? r.json() : null))
        .then((j: { handle?: string | null; avatar?: string } | null) => {
          if (j?.handle) set({ myHandle: j.handle });
          if (j?.avatar !== undefined) set({ myAvatar: j.avatar });
        })
        .catch(() => {});
      // 切断 → 再接続時、サーバが「お前まだこの部屋にいるよ」と返してくる。自動で
      // JoinRoom して復帰する(member 既存なら on_join 側で observation を再送)。
      if (msg.payload.current_room) {
        socket?.send({
          type: "join_room",
          payload: { room_id: msg.payload.current_room },
        });
      }
      break;

    case "room_state": {
      const view = msg.payload.state;
      set({ room: view, roomId: view.room_id });
      // suspended seat 表示はサーバの player.online から推測(suspended = !online)
      const offlineSeats = new Map<number, number>();
      for (const p of view.players as PlayerView[]) {
        if (!p.online && p.seat !== null && p.seat !== undefined) {
          offlineSeats.set(p.seat, Date.now());
        }
      }
      set({ disconnectedSeats: offlineSeats });
      // 再接続時、match_started は再送されないので room_state.players から seats と
      // mySeat を再構築する(初回 match_started がまだなら不正値だが、その場合
      // seat==null なので空配列になる)。
      const me = get().myEmail;
      const seatedPlayers = (view.players as PlayerView[])
        .filter((p): p is PlayerView & { seat: number } => p.seat !== null && p.seat !== undefined)
        .sort((a, b) => a.seat - b.seat);
      if (seatedPlayers.length > 0) {
        const derivedSeats = seatedPlayers.map((p) => ({
          seat: p.seat,
          player_id: p.player_id,
          display_name: p.display_name,
        }));
        const myDerivedSeat = seatedPlayers.find((p) => p.player_id === me)?.seat ?? null;
        set((s) => ({
          seats: s.seats ?? derivedSeats,
          mySeat: s.mySeat ?? myDerivedSeat,
        }));
      }
      break;
    }

    case "match_started": {
      const me = get().myEmail;
      set({
        seats: msg.payload.seats,
        matchSeed: Number(msg.payload.match_seed),
        hands_per_match: msg.payload.hand_count,
        mySeat:
          me === null ? null : (msg.payload.seats.find((s) => s.player_id === me)?.seat ?? null),
        events: [],
        nextReadySeats: [],
        lastScores: null,
        finalScores: null,
      });
      break;
    }

    case "observation":
      set({ observation: msg.payload.observation });
      break;

    case "event":
      set((s) => ({
        events: [...s.events, msg.payload.event].slice(-MAX_EVENT_LOG),
      }));
      break;

    case "chat":
      // 単発の新着。append。重複(再接続時に backlog と被るケース)は seq で除去。
      set((s) => {
        if (s.chatLog.some((e) => e.seq === msg.payload.entry.seq)) return {};
        return { chatLog: [...s.chatLog, msg.payload.entry] };
      });
      break;

    case "chat_backlog":
      // backlog は room 入室時に 1 度送られてくる。既存の chatLog をクリアして置換。
      set({ chatLog: msg.payload.entries });
      break;

    case "hand_ended":
      set({
        lastScores: {
          scores: msg.payload.scores,
          cumulative: msg.payload.cumulative,
        },
        nextReadySeats: [],
      });
      break;

    case "next_hand_ready":
      set({ nextReadySeats: msg.payload.ready_seats });
      break;

    case "match_ended":
      set({ finalScores: msg.payload.final_scores });
      break;

    case "player_disconnected":
      set((s) => {
        const m = new Map(s.disconnectedSeats);
        // since_ms は u64 → bigint。Date 計算は number に揃える。
        m.set(msg.payload.seat, Date.now() - Number(msg.payload.since_ms));
        return { disconnectedSeats: m };
      });
      break;

    case "player_reconnected":
      set((s) => {
        const m = new Map(s.disconnectedSeats);
        m.delete(msg.payload.seat);
        return { disconnectedSeats: m };
      });
      break;

    case "error":
      set((s) => ({
        errors: [
          ...s.errors,
          { code: msg.payload.code, message: msg.payload.message, at: Date.now() },
        ].slice(-50),
      }));
      break;

    case "pong":
      break;
  }
}
