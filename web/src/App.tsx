// Phase E-3: WS observation → GamePC / GameMobile + Overlays。
// observation が無いときは Lobby を出して接続/ルーム/Match 開始を行う。
// dev preview(gallery / mock game / mock overlays)は <DevPreview/> として末尾に残す。

import { useEffect, useRef, useState } from "react";
import { TrumpCard } from "./components/Card";
import { sortHand } from "./components/card-shared";
import { GameMobile } from "./components/GameMobile";
import { GamePC, type GamePcViewModel } from "./components/GamePC";
import {
  BidOverlay,
  EndOverlay,
  type OverlayPhase,
  PartnerOverlay,
  WidowOverlay,
} from "./components/Overlays";
import { Avatar, PlayerBadge } from "./components/Player";
import {
  currentHandIndex,
  declarerName,
  mySeatName,
  selectBidVMs,
  selectEndVm,
  selectGameVm,
  selectOverlayPhase,
} from "./lib/select";
import { useStore } from "./lib/store";
import type { WirePlayedCard, WireTrump } from "./types";

function App() {
  const observation = useStore((s) => s.observation);
  const [layout, setLayout] = useState<"pc" | "mobile">("pc");

  return (
    <div style={pageStyle}>
      <Header layout={layout} setLayout={setLayout} />
      {observation ? <GameStage layout={layout} /> : <LobbyView />}
      <DevPreview />
    </div>
  );
}

function Header({
  layout,
  setLayout,
}: {
  layout: "pc" | "mobile";
  setLayout: (v: "pc" | "mobile") => void;
}) {
  const status = useStore((s) => s.status);
  const myEmail = useStore((s) => s.myEmail);
  const myHandle = useStore((s) => s.myHandle);
  const serverVersion = useStore((s) => s.serverVersion);
  const disconnect = useStore((s) => s.disconnect);

  return (
    <header
      style={{
        display: "flex",
        gap: 12,
        alignItems: "center",
        padding: "10px 16px",
        background: "var(--bg-secondary)",
        borderBottom: "1px solid var(--border-color)",
      }}
    >
      <span
        style={{
          fontFamily: "var(--font-serif)",
          fontSize: 18,
          fontWeight: 600,
        }}
      >
        napo-online
      </span>
      <span style={badgeStyle(status)}>{status}</span>
      {myEmail && <span style={{ color: "var(--text-secondary)", fontSize: 12 }}>{myEmail}</span>}
      {myHandle && (
        <span style={{ color: "var(--text-secondary)", fontSize: 12 }}>名前: {myHandle}</span>
      )}
      {serverVersion && (
        <span style={{ color: "var(--text-muted)", fontSize: 11 }}>server v{serverVersion}</span>
      )}
      <span style={{ flex: 1 }} />
      <div style={{ display: "flex", gap: 4 }}>
        {(["pc", "mobile"] as const).map((v) => (
          <button
            type="button"
            key={v}
            onClick={() => setLayout(v)}
            style={{
              ...btnStyle,
              padding: "4px 10px",
              fontSize: 11,
              background: layout === v ? "var(--accent-secondary)" : "transparent",
              color: layout === v ? "#fff" : "var(--text-primary)",
              border: "1px solid var(--border-color)",
            }}
          >
            {v}
          </button>
        ))}
      </div>
      {status !== "disconnected" && (
        <button type="button" onClick={disconnect} style={btnStyle}>
          切断
        </button>
      )}
    </header>
  );
}

function LobbyView() {
  const status = useStore((s) => s.status);
  const myEmail = useStore((s) => s.myEmail);
  const room = useStore((s) => s.room);
  const finalScores = useStore((s) => s.finalScores);
  const seats = useStore((s) => s.seats);
  const errors = useStore((s) => s.errors);
  const connect = useStore((s) => s.connect);
  const send = useStore((s) => s.send);

  // 名前は localStorage に保存して次回プリフィル(server 側 handle は email key で
  // 永続化されているが、別 device / private window でも再入力不要にしたいため)。
  const [displayName, setDisplayName] = useState(
    () => localStorage.getItem("napo:displayName") ?? "alice",
  );
  const [devEmail, setDevEmail] = useState("alice@local");
  const [roomIdInput, setRoomIdInput] = useState("");

  const isHost = room && myEmail && room.host === myEmail;
  const phaseType = room?.state.type ?? null;

  return (
    <div style={{ padding: "16px", maxWidth: 760, margin: "0 auto" }}>
      {status === "disconnected" && (
        <section style={panelStyle} data-testid="panel-connect">
          <h2 style={h2Style}>接続</h2>
          <div style={rowStyle}>
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="名前"
              style={inputStyle}
              data-testid="input-display-name"
            />
            <input
              value={devEmail}
              onChange={(e) => setDevEmail(e.target.value)}
              placeholder="dev_email"
              style={inputStyle}
              data-testid="input-dev-email"
            />
            <button
              type="button"
              onClick={() => {
                localStorage.setItem("napo:displayName", displayName);
                connect(displayName, devEmail);
              }}
              style={btnStyle}
              data-testid="btn-connect"
            >
              接続
            </button>
          </div>
        </section>
      )}

      {status === "connected" && !room && (
        <section style={panelStyle} data-testid="panel-lobby">
          <h2 style={h2Style}>ルーム</h2>
          <div style={rowStyle}>
            <button
              type="button"
              onClick={() => send({ type: "create_room" })}
              style={btnStyle}
              data-testid="btn-create-room"
            >
              新規作成
            </button>
            <input
              value={roomIdInput}
              onChange={(e) => setRoomIdInput(e.target.value)}
              placeholder="room_id"
              style={inputStyle}
              data-testid="input-room-id"
            />
            <button
              type="button"
              onClick={() => send({ type: "join_room", payload: { room_id: roomIdInput } })}
              style={btnStyle}
              data-testid="btn-join-room"
            >
              参加
            </button>
          </div>
        </section>
      )}

      {room && (
        <section style={panelStyle} data-testid="panel-room" data-phase={phaseType}>
          <h2 style={h2Style}>
            部屋 <span data-testid="text-room-id">{room.room_id}</span>{" "}
            <span style={{ fontSize: 11, color: "var(--text-muted)" }}>· {phaseType}</span>
          </h2>
          <p style={{ color: "var(--text-secondary)", margin: 0, fontSize: 12 }}>
            host: {room.host} ({isHost ? "あなた" : "他"})
          </p>
          <ul style={{ paddingLeft: "1.2rem", marginTop: 8 }} data-testid="list-players">
            {room.players.map((p) => (
              <li
                key={p.player_id}
                data-testid={`list-player-${p.player_id}`}
                style={{ color: p.online ? undefined : "var(--text-muted)", fontSize: 13 }}
              >
                {p.display_name}
                {p.is_com && (
                  <span
                    style={{
                      marginLeft: 4,
                      fontSize: 10,
                      padding: "0 4px",
                      borderRadius: 2,
                      background: "var(--accent-secondary)",
                      color: "white",
                    }}
                  >
                    COM
                  </span>
                )}
                {p.seat !== null && ` · seat${p.seat}`} {!p.online && !p.is_com && "— offline"}
              </li>
            ))}
          </ul>
          <div style={{ ...rowStyle, marginTop: 10 }}>
            {phaseType === "lobby" && isHost && (
              <>
                <button
                  type="button"
                  onClick={() => send({ type: "add_com", payload: { display_name: "" } })}
                  style={btnStyle}
                  data-testid="btn-add-com"
                >
                  + COM 追加
                </button>
                <button
                  type="button"
                  onClick={() => send({ type: "start_match" })}
                  style={btnStyle}
                  data-testid="btn-start-match"
                >
                  Start Match
                </button>
              </>
            )}
            {phaseType === "between_hands" && (
              <button
                type="button"
                onClick={() => send({ type: "next_hand" })}
                style={btnStyle}
                data-testid="btn-next-hand-room"
              >
                Next Hand
              </button>
            )}
            {(phaseType === "playing" || phaseType === "between_hands") && isHost && (
              <button type="button" onClick={() => send({ type: "end_match" })} style={btnStyle}>
                End Match
              </button>
            )}
            <button type="button" onClick={() => send({ type: "leave_room" })} style={btnStyle}>
              退出
            </button>
          </div>
        </section>
      )}

      {finalScores && seats && (
        <section style={panelStyle}>
          <h2 style={h2Style}>最終スコア</h2>
          <ul style={{ paddingLeft: "1.2rem", margin: 0, fontSize: 13 }}>
            {seats.map((s) => (
              <li key={s.seat}>
                seat{s.seat} {s.display_name}: {finalScores[s.seat] ?? 0}
              </li>
            ))}
          </ul>
        </section>
      )}

      {errors.length > 0 && (
        <section style={panelStyle}>
          <h2 style={{ ...h2Style, color: "var(--accent-primary)" }}>エラー</h2>
          <ul style={{ paddingLeft: "1.2rem", margin: 0, fontSize: 13 }}>
            {errors.slice(-5).map((e, i) => (
              <li key={i} style={{ color: "var(--accent-primary)" }}>
                [{e.code}] {e.message}
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/** 5枚揃い表示の hold + 勝者方向への吸い寄せアニメ尺(合計)。CSS と同期。 */
const TRICK_HOLD_DURATION_MS = 700;
const TRICK_GATHER_ANIM_MS = 1000;
const TRICK_GATHER_DURATION_MS = TRICK_HOLD_DURATION_MS + TRICK_GATHER_ANIM_MS;

function GameStage({ layout }: { layout: "pc" | "mobile" }) {
  const observation = useStore((s) => s.observation);
  const room = useStore((s) => s.room);
  const seats = useStore((s) => s.seats);
  const mySeat = useStore((s) => s.mySeat);
  const myEmail = useStore((s) => s.myEmail);
  const hands_per_match = useStore((s) => s.hands_per_match);
  const events = useStore((s) => s.events);
  const finalScores = useStore((s) => s.finalScores);
  const disconnectedSeats = useStore((s) => s.disconnectedSeats);
  const submitAction = useStore((s) => s.submitAction);
  const send = useStore((s) => s.send);

  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);
  const [selectedTricks, setSelectedTricks] = useState(13);
  const [selectedTrump, setSelectedTrump] = useState<WireTrump>("S");
  const [trickAnim, setTrickAnim] = useState<{
    plays: WirePlayedCard[];
    winnerSeat: number;
  } | null>(null);

  // observation.completed.length が増えた瞬間を捉えて gather animation を発火。
  // observation を deps に入れると毎フレーム cleanup → タイマー再生成 で残留バグが起きるので
  // completedCount のみを deps にし、observation は ref で参照する。
  const completedCount = observation?.completed.length ?? 0;
  const lastShownCount = useRef(completedCount);
  const observationRef = useRef(observation);
  observationRef.current = observation;
  useEffect(() => {
    if (completedCount <= lastShownCount.current) {
      lastShownCount.current = completedCount;
      return;
    }
    lastShownCount.current = completedCount;
    const obs = observationRef.current;
    if (!obs) return;
    const latest = obs.completed[completedCount - 1];
    if (!latest) return;
    setTrickAnim({ plays: latest.plays, winnerSeat: latest.winner });
    const t = setTimeout(() => setTrickAnim(null), TRICK_GATHER_DURATION_MS);
    return () => clearTimeout(t);
  }, [completedCount]);

  const handIndex = currentHandIndex(events);
  const vm = selectGameVm({
    observation,
    room,
    seats,
    mySeat,
    myEmail,
    hands_per_match,
    handIndex,
    disconnectedSeats,
    trickAnim,
  });
  const overlay = selectOverlayPhase(observation, events);
  const myName = mySeatName({ seats, mySeat, myEmail });

  if (!vm || !observation || mySeat === null) {
    return (
      <div
        style={{ padding: 16, color: "var(--text-secondary)" }}
        data-testid="game-stage"
        data-overlay="loading"
      >
        observation 待機中…
      </div>
    );
  }

  const onPlay = () => {
    if (selectedIdx === null) return;
    const card = vm.myHand[selectedIdx];
    if (!card) return;
    submitAction({ type: "play_card", payload: { card } });
    setSelectedIdx(null);
  };

  // 選択済みカードを再 click したらそのまま提出。playable でない/自分の番でない場合は
  // submit せず、選択だけトグル。
  const onSelectCard = (i: number) => {
    if (selectedIdx === i && vm.phase === "turn") {
      const card = vm.myHand[i];
      if (card) {
        submitAction({ type: "play_card", payload: { card } });
        setSelectedIdx(null);
        return;
      }
    }
    setSelectedIdx(i);
  };

  return (
    <div
      style={{ padding: 16 }}
      data-testid="game-stage"
      data-overlay={overlay ?? "none"}
      data-my-turn={observation.to_act === mySeat ? "true" : "false"}
      data-completed-tricks={observation.completed.length}
      data-trick-index={observation.trick_index}
      data-trick-anim={trickAnim ? "true" : "false"}
      data-room-phase={room?.state.type ?? "unknown"}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "center",
          background: "#efeae0",
          padding: 12,
          borderRadius: "var(--radius-md)",
        }}
      >
        <div
          style={{
            position: "relative",
            width: layout === "mobile" ? 402 : 1280,
            height: layout === "mobile" ? 720 : 800,
            maxWidth: "100%",
            border: "1px solid var(--border-color)",
            borderRadius: "var(--radius-md)",
            overflow: "hidden",
            background: "var(--bg-primary)",
          }}
        >
          {layout === "mobile" ? (
            <GameMobile
              vm={vm}
              selectedIdx={selectedIdx}
              onSelectCard={onSelectCard}
              onPlay={onPlay}
              myName={myName}
            />
          ) : (
            <GamePC vm={vm} selectedIdx={selectedIdx} onSelectCard={onSelectCard} onPlay={onPlay} />
          )}
          {overlay === "bid" && (
            <BidStageOverlay
              myName={myName}
              selectedTricks={selectedTricks}
              selectedTrump={selectedTrump}
              setSelectedTricks={setSelectedTricks}
              setSelectedTrump={setSelectedTrump}
              mobile={layout === "mobile"}
            />
          )}
          {overlay === "partner" && (
            <PartnerStageOverlay mySeat={mySeat} mobile={layout === "mobile"} />
          )}
          {overlay === "widow" && (
            <WidowStageOverlay
              observation={observation}
              mySeat={mySeat}
              mobile={layout === "mobile"}
            />
          )}
          <HandEndStageOverlay
            handIndex={handIndex}
            hands_per_match={hands_per_match}
            mobile={layout === "mobile"}
            onNext={() => send({ type: "next_hand" })}
          />
          {finalScores && room?.state.type === "ended" && (
            <FinalMatchOverlay finalScores={finalScores} />
          )}
        </div>
      </div>
      <DevControls />
    </div>
  );
}

function HandEndStageOverlay({
  handIndex,
  hands_per_match,
  mobile,
  onNext,
}: {
  handIndex: number | null;
  hands_per_match: number | null;
  mobile?: boolean;
  onNext: () => void;
}) {
  const observation = useStore((s) => s.observation);
  const seats = useStore((s) => s.seats);
  const lastScores = useStore((s) => s.lastScores);
  const room = useStore((s) => s.room);

  const phaseType = room?.state.type;
  const isBetween = phaseType === "between_hands" || phaseType === "ended";
  if (!isBetween) return null;

  const endSel = selectEndVm({ observation, seats, lastScores });
  if (!endSel) return null;

  return (
    <EndOverlay
      mobile={mobile}
      napoTeam={endSel.napoTeam}
      armyTeam={endSel.armyTeam}
      napoWon={endSel.napoWon}
      bidValue={endSel.bidValue}
      taken={endSel.taken}
      standings={endSel.standings}
      currentGame={(handIndex ?? 0) + 1}
      totalGames={hands_per_match ?? 10}
      matchEnded={phaseType === "ended"}
      onNext={onNext}
    />
  );
}

function FinalMatchOverlay({ finalScores }: { finalScores: number[] }) {
  // ended phase で最終 standings を出すための簡易バッジ。EndOverlay 内 standings と重複するので
  // 暫定的に何も表示しない(EndOverlay 側で matchEnded=true による「セット終了」ラベルがある)。
  // finalScores の使用は将来 (Match 集計画面 等) を想定して引数だけ受けておく。
  void finalScores;
  return null;
}

function WidowStageOverlay({
  observation,
  mySeat,
  mobile,
}: {
  observation: import("./types").WireObservation;
  mySeat: number;
  mobile?: boolean;
}) {
  const submitAction = useStore((s) => s.submitAction);
  const isDeclarer = observation.declarer === mySeat;

  // declarer 以外は overlay を出さず、GamePC の wait UI に任せる
  if (!isDeclarer || !observation.visible_widow) return null;

  return (
    <WidowOverlay
      mobile={mobile}
      widow={sortHand(observation.visible_widow)}
      hand={sortHand(observation.hand)}
      discardCount={observation.visible_widow.length}
      onConfirm={(discard) => submitAction({ type: "exchange_widow", payload: { discard } })}
    />
  );
}

function BidStageOverlay({
  myName,
  selectedTricks,
  selectedTrump,
  setSelectedTricks,
  setSelectedTrump,
  mobile,
}: {
  myName: string;
  selectedTricks: number;
  selectedTrump: WireTrump;
  setSelectedTricks: (n: number) => void;
  setSelectedTrump: (s: WireTrump) => void;
  mobile?: boolean;
}) {
  const observation = useStore((s) => s.observation);
  const seats = useStore((s) => s.seats);
  const mySeat = useStore((s) => s.mySeat);
  const submitAction = useStore((s) => s.submitAction);

  const bidSel = selectBidVMs({ observation, seats, mySeat });
  if (!bidSel || !observation || mySeat === null) return null;

  const myTurn = observation.to_act === mySeat;
  const tricks = Math.max(selectedTricks, bidSel.minTricks);

  return (
    <BidOverlay
      mobile={mobile}
      opponents={bidSel.opponents}
      myName={myName}
      currentTopBid={bidSel.currentTopBid}
      myTurn={myTurn}
      minTricks={bidSel.minTricks}
      selectedTricks={tricks}
      selectedTrump={selectedTrump}
      onSelectTricks={setSelectedTricks}
      onSelectTrump={setSelectedTrump}
      onPass={() => submitAction({ type: "pass" })}
      onBid={() =>
        submitAction({
          type: "bid",
          payload: { tricks, trump: selectedTrump },
        })
      }
    />
  );
}

function PartnerStageOverlay({ mySeat, mobile }: { mySeat: number; mobile?: boolean }) {
  const observation = useStore((s) => s.observation);
  const seats = useStore((s) => s.seats);
  const submitAction = useStore((s) => s.submitAction);
  const [picked, setPicked] = useState<string | null>(null);

  const name = declarerName({ observation, seats }) ?? "—";
  if (!observation) return null;

  const amDeclarer = observation.declarer === mySeat;

  return (
    <PartnerOverlay
      mobile={mobile}
      declarerName={name}
      picker={
        amDeclarer
          ? {
              selectedCard: picked,
              onSelect: setPicked,
              onConfirm: () => {
                if (picked) submitAction({ type: "declare_adjutant", payload: { card: picked } });
              },
            }
          : undefined
      }
    />
  );
}

function DevControls() {
  const send = useStore((s) => s.send);
  const lastScores = useStore((s) => s.lastScores);
  const room = useStore((s) => s.room);
  const events = useStore((s) => s.events);
  const errors = useStore((s) => s.errors);

  return (
    <details style={{ marginTop: 12 }}>
      <summary style={{ cursor: "pointer", fontSize: 12, color: "var(--text-secondary)" }}>
        Dev controls / event log
      </summary>
      <div style={{ ...rowStyle, marginTop: 8 }}>
        <button type="button" onClick={() => send({ type: "abort_hand" })} style={btnStyle}>
          AbortHand
        </button>
        <button type="button" onClick={() => send({ type: "next_hand" })} style={btnStyle}>
          NextHand
        </button>
        <button type="button" onClick={() => send({ type: "end_match" })} style={btnStyle}>
          EndMatch
        </button>
        {room && (
          <span style={{ fontSize: 11, color: "var(--text-muted)" }}>
            room.state: {room.state.type}
          </span>
        )}
      </div>
      {lastScores && (
        <div style={{ marginTop: 8, fontSize: 12 }}>
          Last hand scores: [{lastScores.scores.join(", ")}] / cumul: [
          {lastScores.cumulative.join(", ")}]
        </div>
      )}
      <pre
        style={{
          marginTop: 8,
          fontSize: 11,
          background: "var(--bg-secondary)",
          padding: 8,
          borderRadius: "var(--radius-sm)",
          maxHeight: 180,
          overflowY: "auto",
        }}
      >
        {events
          .slice(-30)
          .map((e, i) => `${i}: ${JSON.stringify(e)}`)
          .join("\n")}
      </pre>
      {errors.length > 0 && (
        <ul style={{ paddingLeft: "1.2rem", margin: "8px 0 0", fontSize: 12 }}>
          {errors.slice(-5).map((e, i) => (
            <li key={i} style={{ color: "var(--accent-primary)" }}>
              [{e.code}] {e.message}
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}

// ====================== Dev preview (mock 表示) ======================

function DevPreview() {
  const [open, setOpen] = useState(false);
  return (
    <details
      open={open}
      onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}
      style={{ margin: "16px", padding: 8, background: "var(--bg-secondary)" }}
    >
      <summary style={{ cursor: "pointer", color: "var(--text-secondary)" }}>
        Dev preview (gallery / mock game / mock overlays)
      </summary>
      {open && (
        <>
          <ComponentGallery />
          <GamePreview />
          <OverlayPreview />
        </>
      )}
    </details>
  );
}

function OverlayPreview() {
  const [active, setActive] = useState<OverlayPhase>("bid");
  const [tricks, setTricks] = useState(15);
  const [trump, setTrump] = useState<WireTrump>("H");

  return (
    <section style={{ ...panelStyle, padding: 0, overflow: "hidden" }}>
      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "0.75rem 1rem",
          background: "var(--bg-secondary)",
          borderBottom: "1px solid var(--border-color)",
          alignItems: "center",
        }}
      >
        <h2 style={{ ...h2Style, margin: 0, flex: 1 }}>Overlays preview</h2>
        {(["bid", "partner", "end", null] as const).map((p) => (
          <button
            type="button"
            key={String(p)}
            onClick={() => setActive(p)}
            style={{
              padding: "0.25rem 0.6rem",
              border: "1px solid var(--border-color)",
              borderRadius: "var(--radius-sm)",
              background: active === p ? "var(--accent-secondary)" : "transparent",
              color: active === p ? "white" : "var(--text-primary)",
              cursor: "pointer",
              fontSize: "0.85rem",
              fontFamily: "var(--font-sans)",
            }}
          >
            {p ?? "off"}
          </button>
        ))}
      </div>
      <div
        style={{
          position: "relative",
          width: "100%",
          height: 760,
          background: "var(--bg-primary)",
        }}
      >
        {active === "bid" && (
          <BidOverlay
            opponents={[
              {
                name: "ナギ",
                pos: "tl",
                bid: "12 ハート",
                passed: false,
                leading: false,
                waiting: false,
              },
              {
                name: "リオ",
                pos: "tr",
                bid: "パス",
                passed: true,
                leading: false,
                waiting: false,
              },
              {
                name: "ミナト",
                pos: "bl",
                bid: "14 ハート",
                passed: false,
                leading: true,
                waiting: false,
              },
              { name: "アオイ", pos: "br", bid: "—", passed: false, leading: false, waiting: true },
            ]}
            myName="ハル"
            currentTopBid={{ tricks: 14, trump: "H", by: "ミナト" }}
            myTurn
            minTricks={15}
            selectedTricks={tricks}
            selectedTrump={trump}
            onSelectTricks={setTricks}
            onSelectTrump={setTrump}
            onPass={() => alert("pass")}
            onBid={() => alert(`bid ${tricks} ${trump}`)}
          />
        )}
        {active === "partner" && <PartnerOverlay declarerName="ミナト" />}
        {active === "end" && (
          <EndOverlay
            napoTeam={[
              { name: "ミナト", tricks: 6, picture: 4 },
              { name: "アオイ", tricks: 4, picture: 3, hidden: "partner" },
            ]}
            armyTeam={[
              { name: "ナギ", tricks: 5, picture: 2 },
              { name: "リオ", tricks: 3, picture: 1 },
              { name: "ハル", tricks: 2, picture: 0 },
            ]}
            napoWon
            bidValue={13}
            taken={14}
            standings={[
              { name: "ミナト", total: 7, delta: 2, role: "napo" },
              { name: "アオイ", total: 4, delta: 2, role: "partner" },
              { name: "ナギ", total: -1, delta: -2, role: "army" },
              { name: "リオ", total: -5, delta: -2, role: "army" },
              { name: "ハル", total: -5, delta: -2, role: "army" },
            ]}
            currentGame={4}
            totalGames={10}
            onNext={() => alert("next hand")}
          />
        )}
      </div>
    </section>
  );
}

function GamePreview() {
  const [variant, setVariant] = useState<"pc" | "mobile" | "turn-pc" | "wait-pc">("turn-pc");
  const [selectedIdx, setSelectedIdx] = useState<number | null>(4);

  const baseVm: GamePcViewModel = {
    phase: "turn",
    myHand: ["SA", "SK", "HJ", "H3", "DQ", "D9", "D4"],
    myCaptured: ["HQ"],
    opponents: [
      {
        name: "ナギ",
        pos: "tl",
        depth: "back",
        status: "played",
        cards: 6,
        captured: ["DA", "CA"],
        total: 1,
      },
      {
        name: "リオ",
        pos: "tr",
        depth: "back",
        status: "played",
        cards: 6,
        captured: ["S10", "DJ"],
        total: -3,
      },
      {
        name: "ミナト",
        pos: "bl",
        depth: "front",
        status: "played",
        cards: 6,
        captured: ["DK", "C9"],
        total: 5,
        isNapo: true,
      },
      {
        name: "アオイ",
        pos: "br",
        depth: "front",
        status: "waiting",
        cards: 6,
        captured: ["SQ", "CJ"],
        total: -2,
        isPartner: true,
      },
    ],
    trick: [
      { player: "ミナト", card: "D7", rot: 5, seat: 2, pos: "tl", lead: true },
      { player: "アオイ", card: "CQ", rot: -4, seat: 4, pos: "br" },
      { player: "ナギ", card: "HA", rot: -6, seat: 1, pos: "bl", winner: true },
      { player: "リオ", card: "HK", rot: 4, seat: 3, pos: "tr" },
    ],
    trickPhase: "playing",
    trickWinnerPos: null,
    revealedFaceDiscards: [],
    leadSuit: "D",
    napoName: "ミナト",
    trumpSuit: "H",
    isNoTrump: false,
    bidValue: 13,
    roomId: "NAP-3KF2",
    gameNo: 4,
    totalGames: 10,
    trickNo: 4,
    totalTricks: 10,
  };

  const waitVm: GamePcViewModel = { ...baseVm, phase: "wait" };
  const vm = variant === "wait-pc" ? waitVm : baseVm;
  const isMobile = variant === "mobile";

  return (
    <section style={{ ...panelStyle, padding: 0, overflow: "hidden" }}>
      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "0.75rem 1rem",
          background: "var(--bg-secondary)",
          borderBottom: "1px solid var(--border-color)",
          alignItems: "center",
        }}
      >
        <h2 style={{ ...h2Style, margin: 0, flex: 1 }}>Game preview (mock)</h2>
        {(["turn-pc", "wait-pc", "mobile"] as const).map((v) => (
          <button
            type="button"
            key={v}
            onClick={() => setVariant(v)}
            style={{
              padding: "0.25rem 0.6rem",
              border: "1px solid var(--border-color)",
              borderRadius: "var(--radius-sm)",
              background: variant === v ? "var(--accent-secondary)" : "transparent",
              color: variant === v ? "white" : "var(--text-primary)",
              cursor: "pointer",
              fontSize: "0.85rem",
              fontFamily: "var(--font-sans)",
            }}
          >
            {v}
          </button>
        ))}
      </div>
      <div
        style={{
          width: "100%",
          height: isMobile ? 720 : 800,
          display: "flex",
          justifyContent: "center",
          background: "#efeae0",
          padding: "1rem",
        }}
      >
        <div
          style={{
            width: isMobile ? 402 : 1280,
            maxWidth: "100%",
            height: "100%",
            border: "1px solid var(--border-color)",
            borderRadius: "var(--radius-md)",
            overflow: "hidden",
            background: "var(--bg-primary)",
          }}
        >
          {isMobile ? (
            <GameMobile
              vm={vm}
              selectedIdx={selectedIdx}
              onSelectCard={setSelectedIdx}
              onPlay={() => alert(`play idx=${selectedIdx}`)}
              myName="ハル"
              myTotal={-1}
            />
          ) : (
            <GamePC
              vm={vm}
              selectedIdx={selectedIdx}
              onSelectCard={setSelectedIdx}
              onPlay={() => alert(`play idx=${selectedIdx}`)}
            />
          )}
        </div>
      </div>
    </section>
  );
}

function ComponentGallery() {
  return (
    <>
      <section style={panelStyle}>
        <h2 style={h2Style}>Cards (gallery)</h2>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", paddingTop: 14 }}>
          <TrumpCard code="SA" label="マイティ" highlight="mighty" />
          <TrumpCard code="HQ" label="よろめき" highlight="partner" />
          <TrumpCard code="SJ" label="正J" highlight="jack" />
          <TrumpCard code="DK" />
          <TrumpCard code="C10" />
          <TrumpCard code="H7" />
          <TrumpCard code="D2" />
          <TrumpCard code="JK" />
          <TrumpCard code="SA" faceDown />
          <TrumpCard code="SA" selected />
          <TrumpCard code="SA" dimmed />
        </div>
      </section>

      <section style={panelStyle}>
        <h2 style={h2Style}>Players (gallery)</h2>
        <div style={{ display: "flex", gap: 12, flexWrap: "wrap", alignItems: "flex-start" }}>
          <Avatar name="ハル" size={40} active />
          <Avatar name="ナギ" size={40} team="napo" />
          <Avatar name="リオ" size={40} team="army" />
          <Avatar name="ミナト" size={40} />
          <Avatar name="aoi" size={40} />
        </div>
        <div
          style={{
            display: "flex",
            gap: 16,
            flexWrap: "wrap",
            marginTop: 16,
            paddingTop: 14,
          }}
        >
          <PlayerBadge name="ハル" status="thinking" active />
          <PlayerBadge name="ナギ" status="played" team="napo" isNapo bid="13♠" />
          <PlayerBadge name="リオ" status="pass" team="army" />
          <PlayerBadge name="ミナト" team="napo" isPartner />
          <PlayerBadge name="アオイ" status="waiting" total={-3} />
        </div>
        <div
          style={{
            display: "flex",
            gap: 16,
            flexWrap: "wrap",
            marginTop: 16,
            paddingTop: 14,
            borderTop: "1px dashed var(--border-color)",
          }}
        >
          <PlayerBadge name="ハル" compact horizontal active total={5} bid="13♠" isNapo />
          <PlayerBadge name="ナギ" compact horizontal align="right" total={-2} />
        </div>
      </section>
    </>
  );
}

const pageStyle: React.CSSProperties = {
  minHeight: "100vh",
  fontFamily: "var(--font-sans)",
  color: "var(--text-primary)",
  background: "var(--bg-primary)",
};

const panelStyle: React.CSSProperties = {
  background: "var(--bg-secondary)",
  border: "1px solid var(--border-color)",
  borderRadius: "var(--radius-md)",
  padding: "1rem",
  marginBottom: "1rem",
};

const h2Style: React.CSSProperties = {
  fontSize: "1rem",
  margin: "0 0 0.5rem",
  color: "var(--accent-secondary)",
};

const rowStyle: React.CSSProperties = {
  display: "flex",
  gap: "0.5rem",
  alignItems: "center",
  flexWrap: "wrap",
};

const inputStyle: React.CSSProperties = {
  padding: "0.4rem 0.6rem",
  border: "1px solid var(--border-color)",
  borderRadius: "var(--radius-sm)",
  fontFamily: "var(--font-sans)",
};

const btnStyle: React.CSSProperties = {
  padding: "0.4rem 0.8rem",
  border: "none",
  borderRadius: "var(--radius-sm)",
  background: "var(--accent-secondary)",
  color: "white",
  cursor: "pointer",
  fontFamily: "var(--font-sans)",
};

function badgeStyle(status: string): React.CSSProperties {
  const color =
    status === "connected"
      ? "var(--accent-success)"
      : status === "connecting"
        ? "var(--accent-warning)"
        : "var(--text-muted)";
  return {
    padding: "0.15rem 0.5rem",
    borderRadius: "var(--radius-sm)",
    background: color,
    color: "white",
    fontSize: "0.85rem",
  };
}

export default App;
