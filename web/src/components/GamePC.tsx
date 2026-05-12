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
/** trick の場上配置。自席は "self"(下中央)、他席は四隅。 */
export type FieldSeatPos = CornerPos | "self";

export interface OpponentVM {
  name: string;
  /** emoji 1 文字。空なら initial フォールバック。 */
  avatar?: string;
  pos: CornerPos;
  /** back row(tl/tr)= "back" → 視覚的に縮小、front row(bl/br)= "front" */
  depth: "back" | "front";
  status: "thinking" | "played" | "pass" | "waiting" | "napoleon" | "";
  cards: number;
  captured: string[];
  total?: number;
  isNapo?: boolean;
  isPartner?: boolean;
  /** 切断中(`true` のとき avatar 灰色化 + ラベル表示)。 */
  disconnected?: boolean;
  /** 切断時刻(ms epoch)。経過秒の表示用。 */
  disconnectedSinceMs?: number;
}

export interface FieldPlayVM {
  player: string;
  card: string;
  /** 場のカードの傾き(deg)。 */
  rot: number;
  /** プレイヤーの seat index。 */
  seat: number;
  /** 自席視点での配置(時計回り、自席は "self")。 */
  pos: FieldSeatPos;
  /** lead(その trick の最初に出された)カードか。 */
  lead?: boolean;
  /** 勝者カード(gather animation の集合先 = このカードの位置)。 */
  winner?: boolean;
}

export type GamePhase = "turn" | "wait" | "trick" | "lobby";

/**
 * trick の演出 phase。
 * - "playing": 通常進行(0〜4 枚 current_trick 表示)
 * - "gather": 5 枚揃い、勝者方向へ吸い寄せアニメ中(client side で 1.2s 程度保持)
 */
export type TrickPhase = "playing" | "gather";

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
  /** trick 演出 phase。"gather" の間は trick が「直前 trick の 5 枚」を保持し、勝者へ吸い寄せ。 */
  trickPhase: TrickPhase;
  /** gather 中の勝者の場上位置(trick の最終 sink 先)。 */
  trickWinnerPos: FieldSeatPos | null;
  /** ナポが widow 交換で捨てた絵札(全員に公開)。空なら非表示。 */
  revealedFaceDiscards: string[];
  /** lead suit。マスト・フォロー判定用。 */
  leadSuit: SuitKey | null;
  /** ナポレオンの名前(top bar 表示用)。 */
  napoName: string;
  /** 切り札スート。NT は別途 isNoTrump で表現。 */
  trumpSuit: SuitKey;
  isNoTrump: boolean;
  /** 宣言枚数(13〜20)。 */
  bidValue: number;
  /**
   * 純粋な play phase(bidding / partner / widow を全部抜けた後)か。
   * false の間は「カードを選んでください」等の play 関連 UI を出さない。
   */
  inPlay: boolean;
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

      {/* my-turn warm halo(bid/partner/widow 中は出さない) */}
      {vm.phase === "turn" && vm.inPlay && (
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

      {vm.revealedFaceDiscards.length > 0 && (
        <DiscardedFacesBanner cards={vm.revealedFaceDiscards} />
      )}

      {/* MAIN STAGE — 4-corner opponents around central table */}
      <div
        style={{
          position: "relative",
          zIndex: 3,
          height: `calc(100% - 50px - 250px${vm.revealedFaceDiscards.length > 0 ? " - 56px" : ""})`,
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
        {vm.inPlay && (
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
        )}

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

function DiscardedFacesBanner({ cards }: { cards: string[] }) {
  return (
    <div
      style={{
        position: "relative",
        zIndex: 5,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        padding: "6px 14px",
        background: "rgba(217,83,79,0.08)",
        borderBottom: "1px solid var(--border-color)",
      }}
      data-testid="banner-discarded-faces"
    >
      <span
        style={{
          fontSize: 11,
          letterSpacing: 1,
          color: "var(--accent-primary)",
          fontWeight: 600,
        }}
      >
        ナポが捨てた絵札
      </span>
      <div style={{ display: "flex", gap: 4 }}>
        {cards.map((c) => (
          <TrumpCard key={c} code={c} w={28} h={40} />
        ))}
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
  avatar,
  pos,
  status,
  cards,
  depth,
  captured,
  total,
  isNapo,
  isPartner,
  disconnected,
  disconnectedSinceMs,
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
        avatar={avatar}
        status={status}
        total={total}
        isNapo={isNapo}
        isPartner={isPartner}
        disconnected={disconnected}
        disconnectedSinceMs={disconnectedSinceMs}
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

/** 場の中心からみた各 seat 位置のオフセット (px)。pc layout の Trick 用。 */
const FIELD_LAYOUT_PC: Record<FieldSeatPos, { dx: number; dy: number; scale: number }> = {
  bl: { dx: -96, dy: 60, scale: 1 },
  br: { dx: 96, dy: 60, scale: 1 },
  tl: { dx: -56, dy: -60, scale: 0.9 },
  tr: { dx: 56, dy: -60, scale: 0.9 },
  self: { dx: 0, dy: 90, scale: 1 },
};

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

  // gather 中は勝者位置を集合先にする
  const winnerOff = vm.trickWinnerPos ? FIELD_LAYOUT_PC[vm.trickWinnerPos] : null;
  const isGather = vm.trickPhase === "gather";

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
      {vm.trick.map((p) => {
        const off = FIELD_LAYOUT_PC[p.pos];
        const cw = 72 * off.scale;
        const ch = 104 * off.scale;
        const baseX = cx + off.dx;
        const baseY = cy + off.dy;
        const z = p.winner ? 10 : off.scale >= 1 ? 5 : 2;
        // gather 時の delta: 勝者位置 - 自カード位置
        const gx = isGather && winnerOff ? winnerOff.dx - off.dx : 0;
        const gy = isGather && winnerOff ? winnerOff.dy - off.dy : 0;
        return (
          <div
            key={p.card}
            data-testid={`field-card-${p.card}`}
            style={{
              position: "absolute",
              left: baseX - cw / 2,
              top: baseY - ch / 2,
              // gather 中: 5 枚揃い表示を 700ms hold した後に 1000ms 吸い寄せ。
              animation: isGather
                ? "gather-out 1000ms var(--ease-default) 700ms forwards"
                : "card-land 500ms var(--ease-bounce) backwards",
              zIndex: z,
              filter:
                off.scale < 1
                  ? "drop-shadow(0 4px 6px rgba(43,38,32,0.10))"
                  : "drop-shadow(0 6px 10px rgba(43,38,32,0.16))",
              ...({ "--gx": `${gx}px`, "--gy": `${gy}px` } as CSSProperties),
            }}
          >
            <TrumpCard code={p.card} w={cw} h={ch} rot={p.rot} />
            {p.lead && !isGather && (
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
        // ドーム弧 + 扇形: 中央が最高、両端 baseline。回転は両端が外側に開く向き。
        const maxAbs = (n - 1) / 2;
        const ty = (maxAbs - Math.abs(offset)) * 4;
        const rot = offset * 4.5;
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
