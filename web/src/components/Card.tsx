// tmp/design/cards.jsx を TSX 化。Trump card の SVG 描画 + corner index + face card portrait。
// shared な定数・型は card-shared.ts に分離(only-export-components 対応)。

import type { CSSProperties, MouseEventHandler } from "react";
import {
  type Highlight,
  type SuitKey,
  type SuitMeta,
  PIP_LAYOUTS,
  SUITS,
  isBottomHalf,
  parseCard,
} from "./card-shared";

export interface TrumpCardProps {
  code: string;
  w?: number;
  h?: number;
  faceDown?: boolean;
  highlight?: Highlight;
  selected?: boolean;
  dimmed?: boolean;
  /** Y 方向の浮き上がり(px)。手札の選択ハイライト等。 */
  lift?: number;
  /** 回転角(deg)。fan 配置で利用。 */
  rot?: number;
  onClick?: MouseEventHandler<HTMLDivElement>;
  /** 上に出すラベル。"マイティ" / "正J" 等。 */
  label?: string;
}

export function TrumpCard({
  code,
  w = 88,
  h = 128,
  faceDown = false,
  highlight = null,
  selected = false,
  dimmed = false,
  lift = 0,
  rot = 0,
  onClick,
  label,
}: TrumpCardProps) {
  const { suit, rank } = parseCard(code);
  const cs = w / 88; // 88 を基準にしたスケール

  if (faceDown) {
    return (
      <div
        onClick={onClick}
        style={{
          width: w,
          height: h,
          borderRadius: 6 * cs,
          background: "var(--bg-card-back)",
          border: "1.5px solid var(--text-primary)",
          boxShadow: "var(--shadow-sm)",
          position: "relative",
          overflow: "hidden",
          transform: `translateY(${-lift}px) rotate(${rot}deg)`,
          cursor: onClick ? "pointer" : "default",
          transition: "transform 220ms var(--ease-default)",
        }}
      >
        <div
          style={{
            position: "absolute",
            inset: 6 * cs,
            borderRadius: 3 * cs,
            border: "1px solid rgba(43,38,32,0.3)",
            backgroundImage:
              "repeating-linear-gradient(45deg, rgba(43,38,32,0.18) 0 1px, transparent 1px 6px), repeating-linear-gradient(-45deg, rgba(43,38,32,0.18) 0 1px, transparent 1px 6px)",
          }}
        />
        <div
          style={{
            position: "absolute",
            inset: 0,
            backgroundImage: "var(--noise-url)",
            opacity: 0.18,
            mixBlendMode: "multiply",
          }}
        />
      </div>
    );
  }

  // JK(ジョーカー)は本ゲームでは未使用、code が JK なら裏面相当の薄い表示
  if (suit === "JK") {
    return (
      <div
        onClick={onClick}
        style={{
          width: w,
          height: h,
          borderRadius: 6 * cs,
          background: "var(--bg-secondary)",
          border: "1.5px solid var(--text-muted)",
          color: "var(--text-muted)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontFamily: "var(--font-serif)",
          fontSize: 14 * cs,
        }}
      >
        JK
      </div>
    );
  }

  const s = SUITS[suit];
  const glow =
    highlight === "jack"
      ? "0 0 0 2px #9a8d6e"
      : highlight === "partner"
        ? "0 0 0 2px var(--accent-primary)"
        : null;

  const isFace = ["J", "Q", "K"].includes(rank);
  const isAce = rank === "A";
  const pipLayout = PIP_LAYOUTS[rank];

  return (
    <div
      onClick={onClick}
      style={{
        width: w,
        height: h,
        borderRadius: 6 * cs,
        background: "var(--bg-primary)",
        border: "1.5px solid var(--text-primary)",
        boxShadow: selected
          ? `0 8px 18px rgba(43,38,32,0.22)${glow ? ", " + glow : ""}`
          : `var(--shadow-sm)${glow ? ", " + glow : ""}`,
        position: "relative",
        overflow: "hidden",
        transform: `translateY(${-lift - (selected ? 14 : 0)}px) rotate(${rot}deg)`,
        cursor: onClick ? "pointer" : "default",
        filter: dimmed ? "grayscale(0.95) opacity(0.45) brightness(0.88)" : "none",
        transition: "transform 220ms var(--ease-bounce), box-shadow 180ms ease, filter 180ms",
        fontFamily: "var(--font-serif)",
      }}
    >
      <CornerIndex rank={rank} suit={s} cs={cs} pos="tl" />
      <CornerIndex rank={rank} suit={s} cs={cs} pos="br" />

      {isAce && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: s.color,
            fontSize: 46 * cs,
            lineHeight: 1,
          }}
        >
          {s.sym}
        </div>
      )}

      {isFace && <FaceCard rank={rank} suit={s} cs={cs} />}

      {pipLayout && (
        <div
          style={{
            position: "absolute",
            top: 12 * cs,
            bottom: 12 * cs,
            left: 14 * cs,
            right: 14 * cs,
          }}
        >
          {pipLayout.map(([px, py], i) => (
            <div
              key={i}
              style={{
                position: "absolute",
                left: `${px * 100}%`,
                top: `${py * 100}%`,
                transform: `translate(-50%, -50%) rotate(${isBottomHalf(py) ? 180 : 0}deg)`,
                color: s.color,
                fontSize: 14 * cs,
                lineHeight: 1,
              }}
            >
              {s.sym}
            </div>
          ))}
        </div>
      )}

      {/* paper texture */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          backgroundImage: "var(--noise-url)",
          opacity: "var(--noise-opacity)",
          mixBlendMode: "multiply",
          pointerEvents: "none",
        }}
      />

      {label && (
        <div
          style={{
            position: "absolute",
            top: -8 * cs,
            left: "50%",
            transform: "translateX(-50%)",
            background: highlight === "mighty" ? "#c9a24c" : "#9a8d6e",
            color: "#fff",
            fontSize: 9 * cs,
            fontWeight: 600,
            padding: `${1.5 * cs}px ${5 * cs}px`,
            borderRadius: 3 * cs,
            fontFamily: "var(--font-sans)",
            letterSpacing: 0.3,
            boxShadow: "0 1px 2px rgba(0,0,0,0.2)",
            whiteSpace: "nowrap",
          }}
        >
          {label}
        </div>
      )}
    </div>
  );
}

function CornerIndex({
  rank,
  suit,
  cs,
  pos,
}: {
  rank: string;
  suit: SuitMeta;
  cs: number;
  pos: "tl" | "br";
}) {
  const tl = pos === "tl";
  const style: CSSProperties = {
    position: "absolute",
    [tl ? "top" : "bottom"]: 5 * cs,
    [tl ? "left" : "right"]: 6 * cs,
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    color: suit.color,
    lineHeight: 1,
    transform: tl ? "none" : "rotate(180deg)",
  };
  return (
    <div style={style}>
      <span style={{ fontSize: 15 * cs, fontWeight: 600, letterSpacing: -0.5 }}>{rank}</span>
      <span style={{ fontSize: 11 * cs, marginTop: 1 }}>{suit.sym}</span>
    </div>
  );
}

function FaceCard({ rank, suit, cs }: { rank: string; suit: SuitMeta; cs: number }) {
  return (
    <div
      style={{
        position: "absolute",
        inset: 12 * cs,
        borderRadius: 3 * cs,
        border: `1px solid ${suit.color}`,
        overflow: "hidden",
        background: "rgba(255,255,255,0.18)",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
        <FacePortrait rank={rank} suit={suit} />
      </div>
      <div style={{ height: 1, background: suit.color, opacity: 0.55 }} />
      <div
        style={{
          flex: 1,
          position: "relative",
          overflow: "hidden",
          transform: "rotate(180deg)",
        }}
      >
        <FacePortrait rank={rank} suit={suit} />
      </div>
    </div>
  );
}

function FacePortrait({ rank, suit }: { rank: string; suit: SuitMeta }) {
  const c = suit.color;
  const ink = "var(--text-primary)";
  const skin = "#F2DCC3";
  return (
    <svg
      viewBox="0 0 100 100"
      width="100%"
      height="100%"
      preserveAspectRatio="xMidYMid slice"
      style={{ display: "block" }}
    >
      <rect x="0" y="0" width="100" height="100" fill={c} opacity="0.06" />
      <path
        d="M 10 100 L 22 70 L 78 70 L 90 100 Z"
        fill={c}
        opacity="0.35"
        stroke={ink}
        strokeWidth="0.7"
      />
      <path
        d="M 30 72 L 50 80 L 70 72 L 65 70 L 50 75 L 35 70 Z"
        fill={c}
        opacity="0.7"
        stroke={ink}
        strokeWidth="0.6"
      />
      <ellipse cx="50" cy="48" rx="14" ry="17" fill={skin} stroke={ink} strokeWidth="0.7" />
      <path
        d="M 36 46 Q 32 60 38 64 L 40 56 Z"
        fill={c}
        opacity="0.55"
        stroke={ink}
        strokeWidth="0.5"
      />
      <path
        d="M 64 46 Q 68 60 62 64 L 60 56 Z"
        fill={c}
        opacity="0.55"
        stroke={ink}
        strokeWidth="0.5"
      />
      <line x1="44" y1="46" x2="48" y2="46" stroke={ink} strokeWidth="0.9" strokeLinecap="round" />
      <line x1="52" y1="46" x2="56" y2="46" stroke={ink} strokeWidth="0.9" strokeLinecap="round" />
      <circle cx="46" cy="48" r="0.8" fill={ink} />
      <circle cx="54" cy="48" r="0.8" fill={ink} />
      <path
        d="M 47 56 Q 50 58 53 56"
        fill="none"
        stroke={ink}
        strokeWidth="0.8"
        strokeLinecap="round"
      />

      {rank === "K" && <KingCrown c={c} ink={ink} />}
      {rank === "Q" && <QueenCrown c={c} ink={ink} />}
      {rank === "J" && <JackCap c={c} ink={ink} />}

      {rank === "K" && <Sceptre ink={ink} />}
      {rank === "Q" && <Flower c={c} ink={ink} />}
      {rank === "J" && <Sword ink={ink} />}

      <circle cx="50" cy="86" r="4.5" fill="var(--bg-primary)" stroke={ink} strokeWidth="0.6" />
      <text x="50" y="89" textAnchor="middle" fill={c} fontSize="6" fontFamily="serif">
        {suit.sym}
      </text>
    </svg>
  );
}

function KingCrown({ c, ink }: { c: string; ink: string }) {
  return (
    <g>
      <rect
        x="36"
        y="30"
        width="28"
        height="5"
        fill={c}
        opacity="0.85"
        stroke={ink}
        strokeWidth="0.6"
      />
      <path
        d="M 36 30 L 40 22 L 44 30 L 50 20 L 56 30 L 60 22 L 64 30 Z"
        fill={c}
        opacity="0.85"
        stroke={ink}
        strokeWidth="0.6"
        strokeLinejoin="round"
      />
      <circle cx="40" cy="22" r="1.2" fill="#E0A93B" stroke={ink} strokeWidth="0.4" />
      <circle cx="50" cy="20" r="1.4" fill="#E0A93B" stroke={ink} strokeWidth="0.4" />
      <circle cx="60" cy="22" r="1.2" fill="#E0A93B" stroke={ink} strokeWidth="0.4" />
      <circle cx="50" cy="33" r="1" fill="#E0A93B" stroke={ink} strokeWidth="0.4" />
    </g>
  );
}

function QueenCrown({ c, ink }: { c: string; ink: string }) {
  return (
    <g>
      <path
        d="M 38 32 Q 42 24 46 32 Q 50 22 54 32 Q 58 24 62 32 L 62 35 L 38 35 Z"
        fill={c}
        opacity="0.85"
        stroke={ink}
        strokeWidth="0.6"
        strokeLinejoin="round"
      />
      <circle cx="42" cy="29" r="0.9" fill="#E0A93B" stroke={ink} strokeWidth="0.3" />
      <circle cx="50" cy="26" r="1.1" fill="#E0A93B" stroke={ink} strokeWidth="0.3" />
      <circle cx="58" cy="29" r="0.9" fill="#E0A93B" stroke={ink} strokeWidth="0.3" />
    </g>
  );
}

function JackCap({ c, ink }: { c: string; ink: string }) {
  return (
    <g>
      <path
        d="M 36 34 Q 38 26 50 26 Q 62 26 64 34 L 60 34 Q 58 30 50 30 Q 42 30 40 34 Z"
        fill={c}
        opacity="0.8"
        stroke={ink}
        strokeWidth="0.6"
        strokeLinejoin="round"
      />
      <path
        d="M 60 28 Q 70 22 72 30 Q 66 32 62 30"
        fill={c}
        opacity="0.55"
        stroke={ink}
        strokeWidth="0.5"
      />
      <rect x="36" y="33" width="28" height="2" fill={ink} opacity="0.7" />
    </g>
  );
}

function Sceptre({ ink }: { ink: string }) {
  return (
    <g>
      <line x1="74" y1="92" x2="82" y2="50" stroke={ink} strokeWidth="1.5" strokeLinecap="round" />
      <circle cx="82" cy="48" r="3" fill="#E0A93B" stroke={ink} strokeWidth="0.6" />
      <ellipse cx="73" cy="92" rx="3" ry="2.5" fill="#F2DCC3" stroke={ink} strokeWidth="0.5" />
    </g>
  );
}

function Sword({ ink }: { ink: string }) {
  return (
    <g>
      <line x1="20" y1="92" x2="32" y2="42" stroke={ink} strokeWidth="0.8" />
      <line x1="22" y1="92" x2="34" y2="42" stroke={ink} strokeWidth="0.8" />
      <rect x="26" y="40" width="6" height="2" fill={ink} transform="rotate(-12 29 41)" />
      <rect x="20" y="92" width="10" height="3" fill={ink} />
      <ellipse cx="25" cy="93" rx="3" ry="2.5" fill="#F2DCC3" stroke={ink} strokeWidth="0.5" />
    </g>
  );
}

function Flower({ c, ink }: { c: string; ink: string }) {
  return (
    <g>
      <line x1="22" y1="92" x2="26" y2="58" stroke={ink} strokeWidth="0.8" />
      <path
        d="M 25 72 Q 18 70 20 76 Q 24 76 26 73"
        fill={c}
        opacity="0.5"
        stroke={ink}
        strokeWidth="0.4"
      />
      <circle cx="26" cy="55" r="2.2" fill={c} opacity="0.7" stroke={ink} strokeWidth="0.4" />
      <circle cx="23" cy="57" r="2" fill={c} opacity="0.7" stroke={ink} strokeWidth="0.4" />
      <circle cx="29" cy="57" r="2" fill={c} opacity="0.7" stroke={ink} strokeWidth="0.4" />
      <circle cx="26" cy="59" r="2" fill={c} opacity="0.7" stroke={ink} strokeWidth="0.4" />
      <circle cx="26" cy="57" r="1" fill="#E0A93B" stroke={ink} strokeWidth="0.3" />
      <ellipse cx="22" cy="93" rx="3" ry="2.5" fill="#F2DCC3" stroke={ink} strokeWidth="0.5" />
    </g>
  );
}

/** スート記号だけを描画する小さなヘルパ。 */
export function Suit({ s, size = 14, color }: { s: SuitKey; size?: number; color?: string }) {
  const su = SUITS[s];
  return <span style={{ color: color || su.color, fontSize: size, lineHeight: 1 }}>{su.sym}</span>;
}
