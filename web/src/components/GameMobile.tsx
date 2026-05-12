// tmp/design/game-mobile.jsx を TSX 化。
// iOS 縦長想定の compact layout。
// - 上部 compact bar(room/game/napo/bid)
// - 上部 player row(4 opponents avatar + 各取得 mini fan)
// - 中央 field(round 卓 + 4 plays)
// - 下部 hand(fan)
// - 下部 captured strip(自分の累積取得)
// - 最下部 action bar(出すボタン)
//
// GamePC と同じ GamePcViewModel を再利用(共通の vm)。

import type { CSSProperties } from "react";
import { belongsToLead, type SuitKey, SUITS, parseCard } from "./card-shared";
import { Suit, TrumpCard } from "./Card";
import { Avatar, DisconnectedBadge } from "./Player";
import type { FieldSeatPos, GamePcViewModel } from "./GamePC";

export interface GameMobileProps {
  vm: GamePcViewModel;
  selectedIdx: number | null;
  onSelectCard: (i: number) => void;
  onPlay: () => void;
  myName: string;
  myTotal?: number;
}

export function GameMobile({
  vm,
  selectedIdx,
  onSelectCard,
  onPlay,
  myName,
  myTotal,
}: GameMobileProps) {
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
        display: "flex",
        flexDirection: "column",
      }}
    >
      <PaperGrain />
      {vm.phase === "turn" && vm.inPlay && <TurnHalo />}

      <TopBar vm={vm} />
      <PlayersRow vm={vm} />

      {/* Field area */}
      <div
        style={{
          position: "relative",
          zIndex: 3,
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: "8px 12px",
        }}
      >
        {vm.trick.length > 0 ? <Field vm={vm} /> : <FieldIdle />}
      </div>

      {vm.inPlay && <TurnBanner phase={vm.phase} />}

      {/* Hand */}
      <div
        style={{
          position: "relative",
          zIndex: 4,
          display: "flex",
          justifyContent: "center",
          alignItems: "flex-end",
          height: 110,
          padding: "6px 8px 0",
        }}
      >
        {vm.myHand.map((code, i) => {
          const n = vm.myHand.length;
          const offset = i - (n - 1) / 2;
          const maxAbs = (n - 1) / 2;
          // ドーム弧 + 扇形: 中央が最高、両端は baseline で外側に開く回転。
          const ty = (maxAbs - Math.abs(offset)) * 3;
          const rot = offset * 5;
          const trumpSuitOrNull = vm.isNoTrump ? null : vm.trumpSuit;
          const hasLead =
            vm.leadSuit !== null &&
            vm.myHand.some((c) => belongsToLead(c, vm.leadSuit!, trumpSuitOrNull, vm.isNoTrump));
          const followable =
            !hasLead ||
            (vm.leadSuit !== null &&
              belongsToLead(code, vm.leadSuit, trumpSuitOrNull, vm.isNoTrump));
          const playable = vm.phase === "turn" && followable;
          const isHl = code === "SA" ? "mighty" : code === "HJ" ? "jack" : null;
          return (
            <div
              key={code}
              data-testid={`hand-card-${code}`}
              data-playable={playable ? "true" : "false"}
              data-selected={selectedIdx === i ? "true" : "false"}
              style={{
                marginLeft: i === 0 ? 0 : -22,
                zIndex: selectedIdx === i ? 30 : i,
              }}
            >
              <TrumpCard
                code={code}
                w={62}
                h={92}
                rot={rot}
                lift={ty}
                selected={selectedIdx === i}
                dimmed={!playable}
                highlight={isHl}
                onClick={!playable ? undefined : () => onSelectCard(i)}
              />
            </div>
          );
        })}
      </div>

      {/* My captured strip */}
      <div
        style={{
          position: "relative",
          zIndex: 4,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "4px 12px",
          background: "rgba(242,235,219,0.6)",
          borderTop: "1px solid var(--border-color)",
          gap: 8,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "var(--text-primary)" }}>
            {myName}
          </span>
          {myTotal !== undefined && (
            <span
              style={{
                fontSize: 10,
                fontFamily: "var(--font-serif)",
                color:
                  myTotal > 0
                    ? "var(--accent-success)"
                    : myTotal < 0
                      ? "var(--accent-primary)"
                      : "var(--text-muted)",
                fontWeight: 600,
                background: "var(--bg-secondary)",
                padding: "0 4px",
                borderRadius: 2,
              }}
            >
              {myTotal > 0 ? "+" : ""}
              {myTotal}
            </span>
          )}
          <span style={{ fontSize: 9, color: "var(--text-muted)" }}>あなたの取得</span>
        </div>
        <CapturedPileMini cards={vm.myCaptured} inline />
      </div>

      {/* Bottom action bar */}
      <div
        style={{
          position: "relative",
          zIndex: 5,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "10px 14px",
          background: "var(--bg-secondary)",
          borderTop: "1px solid var(--border-color)",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            fontSize: 11,
            color: "var(--text-secondary)",
          }}
        >
          <span style={{ display: "inline-flex", alignItems: "baseline", gap: 4 }}>
            <span style={{ fontSize: 9, color: "var(--text-muted)", letterSpacing: 0.5 }}>
              GAME
            </span>
            <span
              style={{
                fontFamily: "var(--font-serif)",
                fontSize: 13,
                color: "var(--text-primary)",
                fontWeight: 600,
              }}
            >
              {vm.gameNo}
            </span>
            <span style={{ color: "var(--text-muted)" }}>/</span>
            <span
              style={{ fontFamily: "var(--font-serif)", fontSize: 12, color: "var(--text-muted)" }}
            >
              {vm.totalGames}
            </span>
          </span>
          <span style={{ width: 1, height: 12, background: "var(--border-color)" }} />
          <span style={{ display: "inline-flex", alignItems: "baseline", gap: 4 }}>
            <span style={{ fontSize: 9, color: "var(--text-muted)", letterSpacing: 0.5 }}>
              宣言
            </span>
            <span
              style={{
                fontFamily: "var(--font-serif)",
                fontSize: 13,
                color: "var(--accent-primary)",
                fontWeight: 600,
              }}
            >
              {vm.bidValue}
            </span>
            {vm.isNoTrump ? (
              <span style={{ fontSize: 10 }}>ノートラ</span>
            ) : (
              <Suit s={vm.trumpSuit} size={11} />
            )}
          </span>
        </div>
        <button
          type="button"
          onClick={onPlay}
          disabled={vm.phase !== "turn" || selectedIdx == null}
          data-testid="btn-play"
          style={{
            ...primaryBtn,
            padding: "7px 16px",
            fontSize: 12,
            opacity: vm.phase !== "turn" || selectedIdx == null ? 0.4 : 1,
            cursor: vm.phase !== "turn" || selectedIdx == null ? "not-allowed" : "pointer",
          }}
        >
          出す
        </button>
      </div>
    </div>
  );
}

function PaperGrain() {
  return (
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
  );
}

function TurnHalo() {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        pointerEvents: "none",
        zIndex: 2,
        background:
          "radial-gradient(ellipse at 50% 100%, rgba(217,83,79,0.13) 0%, transparent 60%)",
        animation: "turn-pulse 3s ease-in-out infinite",
      }}
    />
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
        padding: "8px 14px",
        height: 38,
        background: "var(--bg-secondary)",
        borderBottom: "1px solid var(--border-color)",
        fontSize: 11,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontFamily: "var(--font-serif)", fontSize: 13 }}>{vm.roomId}</span>
        <span style={{ color: "var(--text-muted)" }}>·</span>
        <span style={{ fontFamily: "var(--font-serif)", fontSize: 13 }}>
          {vm.gameNo}
          <span style={{ color: "var(--text-muted)", fontSize: 11 }}>/{vm.totalGames}</span>
        </span>
      </div>
      <div
        style={{ display: "flex", alignItems: "center", gap: 8, color: "var(--text-secondary)" }}
      >
        <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
          <span style={{ fontFamily: "var(--font-serif)", fontWeight: 700, fontSize: 14 }}>
            ♛{vm.napoName}
          </span>
          <span style={{ color: "var(--text-muted)" }}>·</span>
          {vm.isNoTrump ? (
            <span style={{ fontSize: 11 }}>ノートラ</span>
          ) : (
            <Suit s={vm.trumpSuit} size={14} />
          )}
          <span
            style={{
              fontFamily: "var(--font-serif)",
              fontWeight: 700,
              fontSize: 15,
              color: "var(--accent-primary)",
            }}
          >
            {vm.bidValue}
          </span>
        </span>
        <span style={{ color: "var(--text-muted)" }}>⚙</span>
      </div>
    </div>
  );
}

function PlayersRow({ vm }: { vm: GamePcViewModel }) {
  return (
    <div
      style={{
        position: "relative",
        zIndex: 4,
        display: "flex",
        justifyContent: "space-around",
        alignItems: "flex-start",
        padding: "12px 8px 8px",
        background: "rgba(242,235,219,0.5)",
      }}
    >
      {vm.opponents.map((o) => (
        <div
          key={o.name}
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 3,
            minWidth: 0,
            flex: "1 1 0",
            maxWidth: 90,
          }}
        >
          <div style={{ position: "relative" }}>
            <Avatar name={o.name} avatar={o.avatar} size={32} disconnected={o.disconnected} />
            {o.isNapo && <SmallCrown />}
            {o.isPartner && <SmallPartnerChip />}
          </div>
          <span
            style={{
              fontSize: 11,
              color: "var(--text-primary)",
              fontWeight: 600,
              maxWidth: "100%",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {o.name}
          </span>
          {o.disconnected && o.disconnectedSinceMs !== undefined && (
            <DisconnectedBadge sinceMs={o.disconnectedSinceMs} />
          )}
          {o.total !== undefined && (
            <span
              style={{
                fontSize: 9,
                fontFamily: "var(--font-serif)",
                color:
                  o.total > 0
                    ? "var(--accent-success)"
                    : o.total < 0
                      ? "var(--accent-primary)"
                      : "var(--text-muted)",
                fontWeight: 600,
                background: "var(--bg-secondary)",
                padding: "0 3px",
                borderRadius: 2,
                lineHeight: 1.5,
              }}
            >
              {o.total > 0 ? "+" : ""}
              {o.total}
            </span>
          )}
          <CapturedPileMini cards={o.captured} />
        </div>
      ))}
    </div>
  );
}

function SmallCrown() {
  return (
    <div
      style={{
        position: "absolute",
        top: -8,
        left: "50%",
        transform: "translateX(-50%)",
        filter: "drop-shadow(0 1px 1px rgba(0,0,0,0.25))",
      }}
    >
      <svg width="18" height="11" viewBox="0 0 22 16">
        <path
          d="M2 14 L4 4 L8 9 L11 2 L14 9 L18 4 L20 14 Z"
          fill="url(#crownGradMobile)"
          stroke="#8a5e10"
          strokeWidth="0.8"
          strokeLinejoin="round"
        />
        <line x1="2" y1="14" x2="20" y2="14" stroke="#8a5e10" strokeWidth="1" />
        <defs>
          <linearGradient id="crownGradMobile" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#f4d96b" />
            <stop offset="100%" stopColor="#d99a2a" />
          </linearGradient>
        </defs>
      </svg>
    </div>
  );
}

function SmallPartnerChip() {
  return (
    <div
      style={{
        position: "absolute",
        top: -5,
        right: -5,
        width: 14,
        height: 14,
        borderRadius: "50%",
        background: "var(--accent-primary)",
        color: "#fff",
        fontSize: 9,
        fontWeight: 700,
        fontFamily: "var(--font-sans)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        border: "1.5px solid var(--bg-primary)",
        boxShadow: "0 1px 2px rgba(0,0,0,0.25)",
      }}
    >
      副
    </div>
  );
}

function FieldIdle() {
  return (
    <div
      style={{
        width: 260,
        height: 150,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "var(--bg-table)",
        borderRadius: "var(--radius-lg)",
        border: "1px dashed rgba(43,38,32,0.18)",
        color: "var(--text-muted)",
        fontSize: 12,
      }}
    >
      場 — 待機中
    </div>
  );
}

const FIELD_LAYOUT_MOBILE: Record<FieldSeatPos, { dx: number; dy: number }> = {
  bl: { dx: -68, dy: 12 },
  br: { dx: 68, dy: 12 },
  tl: { dx: -34, dy: -28 },
  tr: { dx: 34, dy: -28 },
  self: { dx: 0, dy: 56 },
};

function Field({ vm }: { vm: GamePcViewModel }) {
  const W = 280;
  const H = 200;
  const cx = W / 2;
  const cy = H / 2;
  const winnerOff = vm.trickWinnerPos ? FIELD_LAYOUT_MOBILE[vm.trickWinnerPos] : null;
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
        boxShadow: "inset 0 1px 3px rgba(43,38,32,0.08)",
      }}
    >
      {vm.trick.map((p) => {
        const off = FIELD_LAYOUT_MOBILE[p.pos];
        const baseX = cx + off.dx;
        const baseY = cy + off.dy;
        const gx = isGather && winnerOff ? winnerOff.dx - off.dx : 0;
        const gy = isGather && winnerOff ? winnerOff.dy - off.dy : 0;
        return (
          <div
            key={p.card}
            data-testid={`field-card-${p.card}`}
            style={
              {
                position: "absolute",
                left: baseX - 26,
                top: baseY - 39,
                animation: isGather
                  ? "gather-out 1000ms var(--ease-default) 700ms forwards"
                  : "card-land 500ms var(--ease-bounce) backwards",
                zIndex: p.winner ? 10 : 5,
                filter: "drop-shadow(0 4px 6px rgba(43,38,32,0.12))",
                "--gx": `${gx}px`,
                "--gy": `${gy}px`,
              } as CSSProperties
            }
          >
            <TrumpCard code={p.card} w={52} h={78} rot={p.rot} />
            {p.lead && !isGather && (
              <div
                style={{
                  position: "absolute",
                  top: -7,
                  left: "50%",
                  transform: `translateX(-50%) rotate(${p.rot}deg)`,
                  background: "var(--text-primary)",
                  color: "#fff",
                  fontSize: 7,
                  fontWeight: 700,
                  letterSpacing: 0.8,
                  padding: "1px 5px",
                  borderRadius: 2,
                  fontFamily: "var(--font-sans)",
                  whiteSpace: "nowrap",
                  boxShadow: "0 1px 2px rgba(0,0,0,0.2)",
                }}
              >
                LEAD
              </div>
            )}
            <div
              style={{
                position: "absolute",
                bottom: -14,
                left: "50%",
                transform: "translateX(-50%)",
                fontSize: 9,
                color: "var(--text-secondary)",
                whiteSpace: "nowrap",
                maxWidth: 56,
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {p.player}
            </div>
          </div>
        );
      })}
      {vm.leadSuit && (
        <div
          style={{
            position: "absolute",
            top: 8,
            left: 14,
            fontSize: 9,
            color: "var(--text-muted)",
            letterSpacing: 0.5,
          }}
        >
          LEAD <Suit s={vm.leadSuit} size={10} />
        </div>
      )}
    </div>
  );
}

function TurnBanner({ phase }: { phase: GamePcViewModel["phase"] }) {
  return (
    <div
      style={{
        position: "relative",
        zIndex: 4,
        display: "flex",
        justifyContent: "center",
        padding: "4px 0",
      }}
    >
      <div
        style={{
          padding: "5px 16px",
          background: phase === "turn" ? "var(--accent-primary)" : "var(--bg-secondary)",
          color: phase === "turn" ? "#fff" : "var(--text-secondary)",
          borderRadius: 99,
          fontSize: 11,
          fontWeight: 600,
          display: "flex",
          alignItems: "center",
          gap: 6,
          boxShadow: phase === "turn" ? "0 3px 8px rgba(217,83,79,0.3)" : "none",
        }}
      >
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: phase === "turn" ? "#fff" : "var(--text-muted)",
          }}
        />
        {phase === "turn" ? "あなたのターン" : "相手のターン…"}
      </div>
    </div>
  );
}

function CapturedPileMini({ cards, inline = false }: { cards: string[]; inline?: boolean }) {
  if (cards.length === 0) {
    return <div style={{ height: inline ? 22 : 26 }} />;
  }
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
      <div style={{ display: "flex" }}>
        {cards.map((c, i) => (
          <div key={`${c}-${i}`} style={{ marginLeft: i === 0 ? 0 : -12, zIndex: i }}>
            <MiniCardM code={c} />
          </div>
        ))}
      </div>
      {!inline && (
        <span
          style={{
            fontSize: 8,
            color: "var(--text-muted)",
            fontWeight: 600,
            letterSpacing: 0.3,
          }}
        >
          {cards.length}
        </span>
      )}
    </div>
  );
}

function MiniCardM({ code }: { code: string }) {
  const { suit, rank } = parseCard(code);
  if (suit === "JK") return null;
  const s = SUITS[suit];
  return (
    <div
      style={{
        width: 20,
        height: 26,
        background: "var(--bg-primary)",
        border: "0.75px solid var(--text-primary)",
        borderRadius: 2,
        position: "relative",
        boxShadow: "0 1px 1px rgba(43,38,32,0.12)",
        fontFamily: "var(--font-serif)",
        lineHeight: 1,
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 1.5,
          left: 2,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          color: s.color,
        }}
      >
        <span style={{ fontSize: 9, fontWeight: 600, letterSpacing: -0.4 }}>{rank}</span>
        <span style={{ fontSize: 7 }}>{s.sym}</span>
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

// SuitKey is referenced via parseCard return type
export type { SuitKey };
