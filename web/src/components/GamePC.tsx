// tmp/design/game-pc.jsx を TSX 化。
// PC layout(1280x800 想定): 4人対面が四隅、自分の手札が下端 fan、中央に table と current trick。
//
// data は Phase D-1 では props 経由で渡す(viewModel)。
// Phase E で Zustand store + observation から viewModel を派生させる。

import type { CSSProperties } from "react";
import { belongsToLead, type SuitKey, SUITS, parseCard } from "./card-shared";
import { Suit, TrumpCard } from "./Card";
import { PlayerBadge } from "./Player";

export type CornerPos = "tl" | "tr" | "bl" | "br";

export interface OpponentVM {
  name: string;
  pos: CornerPos;
  /** back row(tl/tr)= "back" → 視覚的に縮小、front row(bl/br)= "front" */
  depth: "back" | "front";
  status: "thinking" | "played" | "pass" | "waiting" | "napoleon" | "";
  cards: number;
  captured: string[];
  total?: number;
  isNapo?: boolean;
  isPartner?: boolean;
}

export interface FieldPlayVM {
  player: string;
  card: string;
  /** 場のカードの傾き(deg)。 */
  rot: number;
  /** lead(その trick の最初に出された)カードか。 */
  lead?: boolean;
  /** trick 取得後のアニメーションで「勝者」のカード。 */
  winner?: boolean;
}

export type GamePhase = "turn" | "wait" | "trick" | "lobby";

export interface GamePcViewModel {
  /** ゲームの大局 phase。turn = 自分の番、wait = 他人の番、trick = 場のカードを集めるアニメ中。 */
  phase: GamePhase;
  /** 自分の手札(suit-rank、例 "SA")。 */
  myHand: string[];
  /** 自分が累積で取った絵札(face cards captured)。 */
  myCaptured: string[];
  opponents: OpponentVM[];
  /** 場に出ている current trick(0..5枚)。 */
  trick: FieldPlayVM[];
  /** lead suit。マスト・フォロー判定用。 */
  leadSuit: SuitKey | null;
  /** ナポレオンの名前(top bar 表示用)。 */
  napoName: string;
  /** 切り札スート。NT は別途 isNoTrump で表現。 */
  trumpSuit: SuitKey;
  isNoTrump: boolean;
  /** 宣言枚数(13〜20)。 */
  bidValue: number;
  /** ヘッダー情報。 */
  roomId: string;
  gameNo: number;
  totalGames: number;
  trickNo: number;
  totalTricks: number;
}

export interface GamePCProps {
  vm: GamePcViewModel;
  selectedIdx: number | null;
  onSelectCard: (i: number) => void;
  onPlay: () => void;
}

export function GamePC({ vm, selectedIdx, onSelectCard, onPlay }: GamePCProps) {
  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        position: "relative",
        background: "var(--bg-primary)",
        overflow: "hidden",
        fontFamily: "var(--font-sans)",
        color: "var(--text-primary)",
      }}
    >
      {/* paper grain overlay */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          pointerEvents: "none",
          zIndex: 1,
          backgroundImage: "var(--noise-url)",
          opacity: "var(--noise-opacity)",
          mixBlendMode: "multiply",
        }}
      />

      {/* my-turn warm halo */}
      {vm.phase === "turn" && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            pointerEvents: "none",
            zIndex: 2,
            background:
              "radial-gradient(ellipse at 50% 100%, rgba(217,83,79,0.12) 0%, transparent 55%)",
            animation: "turn-pulse 3s ease-in-out infinite",
          }}
        />
      )}

      <TopBar vm={vm} />

      {/* MAIN STAGE — 4-corner opponents around central table */}
      <div
        style={{
          position: "relative",
          zIndex: 3,
          height: "calc(100% - 50px - 250px)",
          padding: "16px 40px 0",
          boxSizing: "border-box",
        }}
      >
        <div
          style={{
            position: "relative",
            width: "100%",
            height: "100%",
            maxWidth: 880,
            margin: "0 auto",
          }}
        >
          {vm.opponents.map((o) => (
            <CornerOpponent key={o.name} {...o} />
          ))}

          <div
            style={{
              position: "absolute",
              left: "50%",
              top: "50%",
              transform: "translate(-50%, -50%)",
            }}
          >
            <Trick vm={vm} />
          </div>
        </div>
      </div>

      {/* MY HAND AREA */}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: 8,
          zIndex: 4,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 14,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "6px 18px",
            background: vm.phase === "turn" ? "var(--accent-primary)" : "var(--bg-secondary)",
            color: vm.phase === "turn" ? "#fff" : "var(--text-secondary)",
            borderRadius: 99,
            fontSize: 13,
            fontWeight: 600,
            letterSpacing: 0.5,
            boxShadow: vm.phase === "turn" ? "0 4px 12px rgba(217,83,79,0.35)" : "none",
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: vm.phase === "turn" ? "#fff" : "var(--text-muted)",
            }}
          />
          {vm.phase === "turn" ? "あなたのターン — カードを選んでください" : "相手のターンです…"}
        </div>

        <Hand
          hand={vm.myHand}
          selectedIdx={selectedIdx}
          onSelect={onSelectCard}
          disabled={vm.phase !== "turn"}
          leadSuit={vm.leadSuit}
          trumpSuit={vm.trumpSuit}
          isNoTrump={vm.isNoTrump}
        />

        <button
          type="button"
          onClick={onPlay}
          disabled={vm.phase !== "turn" || selectedIdx == null}
          data-testid="btn-play"
          style={{
            ...primaryBtn,
            opacity: vm.phase !== "turn" || selectedIdx == null ? 0.4 : 1,
            cursor: vm.phase !== "turn" || selectedIdx == null ? "not-allowed" : "pointer",
          }}
        >
          このカードを出す
        </button>
      </div>

      {/* My captured pile — bottom-right near my hand */}
      <div style={{ position: "absolute", right: 22, bottom: 8, zIndex: 4 }}>
        <CapturedPile cards={vm.myCaptured} align="right" />
      </div>
    </div>
  );
}

function TopBar({ vm }: { vm: GamePcViewModel }) {
  return (
    <div
      style={{
        position: "relative",
        zIndex: 5,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "12px 22px",
        height: 50,
        boxSizing: "border-box",
        borderBottom: "1px solid var(--border-color)",
        background: "var(--bg-secondary)",
        whiteSpace: "nowrap",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 16, whiteSpace: "nowrap" }}>
        <Cell label="ROOM" value={vm.roomId} mono />
        <Sep />
        <Cell label="GAME" value={`${vm.gameNo}`} suffix={`/${vm.totalGames}`} />
        <Sep />
        <Cell label="TRICK" value={`${vm.trickNo}`} suffix={`/${vm.totalTricks}`} />
        <Sep />
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ fontSize: 11, color: "var(--text-muted)", letterSpacing: 1 }}>NAPO</span>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 4, fontSize: 13 }}>
            <span style={{ fontSize: 14 }}>♛</span>
            {vm.napoName}
          </span>
          <span style={{ color: "var(--text-muted)", fontSize: 12 }}>·</span>
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              fontFamily: "var(--font-serif)",
              fontWeight: 700,
              fontSize: 18,
              color: "var(--text-primary)",
            }}
          >
            {vm.isNoTrump ? (
              <span style={{ fontSize: 13 }}>ノートラ</span>
            ) : (
              <Suit s={vm.trumpSuit} size={18} />
            )}
          </span>
          <span
            style={{
              fontFamily: "var(--font-serif)",
              fontWeight: 700,
              fontSize: 18,
              color: "var(--accent-primary)",
              letterSpacing: -0.5,
            }}
          >
            {vm.bidValue}
          </span>
        </div>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
        <span style={{ fontSize: 11, color: "var(--text-muted)" }}>
          マイティ最強 · 裏J有 · ジョーカー無
        </span>
        <button type="button" style={iconBtn}>
          ⚙
        </button>
      </div>
    </div>
  );
}

function Cell({
  label,
  value,
  suffix,
  mono,
}: {
  label: string;
  value: string;
  suffix?: string;
  mono?: boolean;
}) {
  return (
    <div style={{ display: "flex", alignItems: "baseline", gap: 6 }}>
      <span style={{ fontSize: 11, color: "var(--text-muted)", letterSpacing: 1 }}>{label}</span>
      <span
        style={{
          fontFamily: mono ? "var(--font-serif)" : "var(--font-serif)",
          fontSize: 18,
          color: "var(--text-primary)",
          letterSpacing: 0.5,
        }}
      >
        {value}
        {suffix && <span style={{ color: "var(--text-muted)", fontSize: 14 }}>{suffix}</span>}
      </span>
    </div>
  );
}

function Sep() {
  return <div style={{ width: 1, height: 18, background: "var(--border-color)" }} />;
}

function CornerOpponent({
  name,
  pos,
  status,
  cards,
  depth,
  captured,
  total,
  isNapo,
  isPartner,
}: OpponentVM) {
  const isBack = depth === "back";
  const inset = isBack ? "18%" : 0;
  const POSITIONS: Record<
    CornerPos,
    {
      side1: "top" | "bottom";
      v1: 0;
      side2: "left" | "right";
      v2: string | 0;
      align: "flex-start" | "flex-end";
    }
  > = {
    tl: { side1: "top", v1: 0, side2: "left", v2: inset, align: "flex-start" },
    tr: { side1: "top", v1: 0, side2: "right", v2: inset, align: "flex-end" },
    bl: { side1: "bottom", v1: 0, side2: "left", v2: 0, align: "flex-start" },
    br: { side1: "bottom", v1: 0, side2: "right", v2: 0, align: "flex-end" },
  };
  const p = POSITIONS[pos];
  const scale = isBack ? 0.82 : 1;
  const cardW = 38 * scale;
  const cardH = 56 * scale;
  const dropShadow = isBack
    ? "drop-shadow(0 6px 8px rgba(43,38,32,0.12))"
    : "drop-shadow(0 2px 3px rgba(43,38,32,0.08))";
  const isRight = pos === "tr" || pos === "br";
  const transformOrigin: CSSProperties["transformOrigin"] =
    pos === "tl"
      ? "top left"
      : pos === "tr"
        ? "top right"
        : pos === "bl"
          ? "bottom left"
          : "bottom right";
  return (
    <div
      style={{
        position: "absolute",
        [p.side1]: p.v1,
        [p.side2]: p.v2,
        display: "flex",
        flexDirection: "column",
        alignItems: p.align,
        gap: 8 * scale,
        transform: `scale(${scale})`,
        transformOrigin,
        filter: dropShadow,
      }}
    >
      <div style={{ display: "flex", height: cardH + 4, alignSelf: p.align }}>
        {Array.from({ length: Math.min(cards, 5) }).map((_, i) => {
          const n = Math.min(cards, 5);
          const offset = i - (n - 1) / 2;
          return (
            <div
              key={i}
              style={{
                marginLeft: i === 0 ? 0 : -22 * scale,
                transform: `rotate(${offset * 5}deg)`,
                transformOrigin: "bottom center",
              }}
            >
              <TrumpCard code="JK" faceDown w={cardW} h={cardH} />
            </div>
          );
        })}
      </div>
      <PlayerBadge
        name={name}
        status={status}
        total={total}
        isNapo={isNapo}
        isPartner={isPartner}
        compact
        horizontal
        align={isRight ? "right" : "left"}
      />
      {captured.length > 0 && <CapturedPile cards={captured} align={isRight ? "right" : "left"} />}
    </div>
  );
}

/** プレイヤーが取った絵札の小さい overlapping fan。 */
export function CapturedPile({
  cards,
  align = "left",
}: {
  cards: string[];
  align?: "left" | "right";
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: align === "right" ? "flex-end" : "flex-start",
        gap: 3,
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: align === "right" ? "row-reverse" : "row",
        }}
      >
        {cards.map((c, i) => (
          <div
            key={`${c}-${i}`}
            style={{
              marginLeft: align === "left" && i > 0 ? -16 : 0,
              marginRight: align === "right" && i > 0 ? -16 : 0,
              zIndex: align === "right" ? cards.length - i : i,
            }}
          >
            <MiniCapturedCard code={c} />
          </div>
        ))}
      </div>
      <span
        style={{
          fontSize: 9,
          color: "var(--text-muted)",
          letterSpacing: 0.4,
          fontWeight: 600,
          fontFamily: "var(--font-sans)",
        }}
      >
        取得 {cards.length}
      </span>
    </div>
  );
}

function Trick({ vm }: { vm: GamePcViewModel }) {
  if (vm.trick.length === 0) {
    return (
      <div
        style={{
          width: 360,
          height: 220,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background:
            "radial-gradient(ellipse at center, var(--bg-table) 0%, var(--bg-secondary) 95%)",
          borderRadius: "50%",
          border: "1px dashed rgba(43,38,32,0.18)",
          color: "var(--text-muted)",
          fontSize: 13,
        }}
      >
        場 — 待機中
      </div>
    );
  }

  const W = 380;
  const H = 240;
  const cx = W / 2;
  const cy = H / 2;
  const dxBack = 56;
  const dxFront = 96;
  const dyBack = 60;
  const dyFront = 60;

  // 各 play を seat 位置(対応する opponent.pos)から左右上下にオフセット。
  // 一番手前の自分(pos === undefined)は cy + 中央寄り、対戦相手は四隅。
  const playPos = (idx: number, total: number) => {
    // simple: positions for up to 5 plays, mirror corners
    const layouts = [
      { x: cx - dxFront, y: cy + dyFront, scale: 1 }, // bl
      { x: cx + dxFront, y: cy + dyFront, scale: 1 }, // br
      { x: cx - dxBack, y: cy - dyBack, scale: 0.9 }, // tl
      { x: cx + dxBack, y: cy - dyBack, scale: 0.9 }, // tr
      { x: cx, y: cy + dyFront + 30, scale: 1 }, // self center bottom
    ];
    return layouts[idx] ?? layouts[total - 1];
  };

  return (
    <div
      style={{
        position: "relative",
        width: W,
        height: H,
        background:
          "radial-gradient(ellipse at center, var(--bg-table) 0%, var(--bg-secondary) 95%)",
        borderRadius: "50%",
        boxShadow: "inset 0 1px 3px rgba(43,38,32,0.08), inset 0 0 30px rgba(43,38,32,0.04)",
        overflow: "visible",
      }}
    >
      {vm.trick.map((p, i) => {
        const pos = playPos(i, vm.trick.length);
        const cw = 72 * pos.scale;
        const ch = 104 * pos.scale;
        const z = vm.phase === "trick" && p.winner ? 10 : pos.scale >= 1 ? 5 : 2;
        return (
          <div
            key={`${p.player}-${i}`}
            style={{
              position: "absolute",
              left: pos.x - cw / 2,
              top: pos.y - ch / 2,
              animation:
                vm.phase === "trick"
                  ? `gather 700ms var(--ease-default) ${300 + i * 40}ms forwards`
                  : `card-land 280ms var(--ease-bounce) ${i * 60}ms backwards`,
              zIndex: z,
              filter:
                pos.scale < 1
                  ? "drop-shadow(0 4px 6px rgba(43,38,32,0.10))"
                  : "drop-shadow(0 6px 10px rgba(43,38,32,0.16))",
              // CSS vars for keyframes
              ...({ "--gx": "-260px", "--gy": "-180px" } as CSSProperties),
            }}
          >
            <TrumpCard code={p.card} w={cw} h={ch} rot={p.rot} />
            {p.lead && vm.phase !== "trick" && (
              <div
                style={{
                  position: "absolute",
                  top: -8,
                  left: "50%",
                  transform: `translateX(-50%) rotate(${p.rot}deg)`,
                  background: "var(--text-primary)",
                  color: "#fff",
                  fontSize: 9,
                  fontWeight: 700,
                  letterSpacing: 1,
                  padding: "2px 7px",
                  borderRadius: 3,
                  fontFamily: "var(--font-sans)",
                  boxShadow: "0 1px 3px rgba(0,0,0,0.25)",
                  whiteSpace: "nowrap",
                }}
              >
                LEAD
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function Hand({
  hand,
  selectedIdx,
  onSelect,
  disabled,
  leadSuit,
  trumpSuit,
  isNoTrump,
}: {
  hand: string[];
  selectedIdx: number | null;
  onSelect: (i: number) => void;
  disabled: boolean;
  leadSuit: SuitKey | null;
  trumpSuit: SuitKey;
  isNoTrump: boolean;
}) {
  const n = hand.length;
  const hasLead =
    leadSuit !== null &&
    hand.some((c) => belongsToLead(c, leadSuit, isNoTrump ? null : trumpSuit, isNoTrump));
  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        justifyContent: "center",
        alignItems: "flex-end",
        height: 140,
        paddingBottom: 6,
      }}
    >
      {hand.map((code, i) => {
        const offset = i - (n - 1) / 2;
        const rot = offset * 4.5;
        const ty = Math.abs(offset) * 4;
        // standard Napoleon: must follow lead suit if held(裏 J ルール込み)。
        const playable =
          !leadSuit ||
          !hasLead ||
          belongsToLead(code, leadSuit, isNoTrump ? null : trumpSuit, isNoTrump);
        const isHighlight = code === "SA" ? "mighty" : code === "HJ" ? "jack" : null;

        return (
          <div
            key={code}
            data-testid={`hand-card-${code}`}
            data-playable={playable && !disabled ? "true" : "false"}
            data-selected={selectedIdx === i ? "true" : "false"}
            style={{
              marginLeft: i === 0 ? 0 : -28,
              zIndex: selectedIdx === i ? 30 : i,
            }}
          >
            <TrumpCard
              code={code}
              w={82}
              h={120}
              rot={rot}
              lift={ty}
              selected={selectedIdx === i}
              dimmed={disabled || !playable}
              highlight={isHighlight}
              onClick={disabled ? undefined : () => playable && onSelect(i)}
            />
          </div>
        );
      })}
    </div>
  );
}

function MiniCapturedCard({ code }: { code: string }) {
  const { suit, rank } = parseCard(code);
  if (suit === "JK") return null;
  const s = SUITS[suit];
  return (
    <div
      style={{
        width: 26,
        height: 34,
        background: "var(--bg-primary)",
        border: "1px solid var(--text-primary)",
        borderRadius: 3,
        position: "relative",
        boxShadow: "0 1px 1.5px rgba(43,38,32,0.15)",
        fontFamily: "var(--font-serif)",
        lineHeight: 1,
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 2,
          left: 3,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          color: s.color,
        }}
      >
        <span style={{ fontSize: 11, fontWeight: 600, letterSpacing: -0.5 }}>{rank}</span>
        <span style={{ fontSize: 8, marginTop: 0 }}>{s.sym}</span>
      </div>
    </div>
  );
}

const primaryBtn: CSSProperties = {
  background: "var(--accent-primary)",
  color: "#fff",
  border: "none",
  borderRadius: "var(--radius-md)",
  padding: "10px 24px",
  fontSize: 14,
  fontWeight: 600,
  fontFamily: "var(--font-sans)",
  boxShadow: "0 2px 6px rgba(217,83,79,0.3)",
  cursor: "pointer",
  transition: "transform 120ms, box-shadow 120ms",
  letterSpacing: 0.5,
};

const iconBtn: CSSProperties = {
  width: 28,
  height: 28,
  border: "none",
  background: "transparent",
  color: "var(--text-secondary)",
  fontSize: 16,
  cursor: "pointer",
  borderRadius: 6,
};
