// tmp/design/players.jsx を TSX 化。
// Avatar(円形 + initial + active halo)と PlayerBadge(avatar + name + bid/count + napo/副官マーク)。
//
// shared な定数・関数は player-shared.ts に分離(only-export-components 対応)。

import { useEffect, useState } from "react";
import { type PlayerStatus, type Team, STATUS_TEXT, colorForName } from "./player-shared";

export interface AvatarProps {
  name: string;
  size?: number;
  team?: Team;
  active?: boolean;
  ringColor?: string;
  /** 切断中:grayscale + 透明度を下げる。active より優先。 */
  disconnected?: boolean;
}

export function Avatar({
  name,
  size = 40,
  team = null,
  active = false,
  ringColor,
  disconnected = false,
}: AvatarProps) {
  const initial = name[0] ?? "?";
  const bg = colorForName(name);
  const ring = active
    ? "var(--accent-primary)"
    : team === "napo"
      ? "var(--accent-primary)"
      : team === "army"
        ? "var(--accent-secondary)"
        : (ringColor ?? "transparent");
  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: "50%",
        background: bg,
        color: "#fff",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: "var(--font-sans)",
        fontWeight: 600,
        fontSize: size * 0.42,
        boxShadow:
          ring !== "transparent"
            ? `0 0 0 2.5px var(--bg-primary), 0 0 0 4.5px ${ring}, 0 1px 3px rgba(0,0,0,0.18)`
            : "0 1px 3px rgba(0,0,0,0.18)",
        flexShrink: 0,
        // 切断中は active 演出を止め、grayscale + 半透明に。
        animation: disconnected ? "none" : active ? "pulse-ring 1.6s ease-in-out infinite" : "none",
        filter: disconnected ? "grayscale(1)" : "none",
        opacity: disconnected ? 0.5 : 1,
        transition: "opacity 200ms, filter 200ms",
      }}
    >
      {initial}
    </div>
  );
}

/**
 * 切断中ラベル。`sinceMs` から経過秒を 1 秒間隔で更新表示。
 * 60s 以下は「切断中 N 秒」、それ以上は「切断中 M 分」。
 */
export function DisconnectedBadge({ sinceMs }: { sinceMs: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  const elapsedSec = Math.max(0, Math.floor((now - sinceMs) / 1000));
  const label =
    elapsedSec < 60 ? `切断中 ${elapsedSec}秒` : `切断中 ${Math.floor(elapsedSec / 60)}分`;
  return (
    <span
      style={{
        fontSize: 9,
        fontWeight: 600,
        letterSpacing: 0.4,
        padding: "1px 5px",
        borderRadius: 3,
        background: "var(--text-muted)",
        color: "#fff",
      }}
    >
      {label}
    </span>
  );
}

export interface PlayerBadgeProps {
  name: string;
  /** 状態テキスト。空文字で非表示。 */
  status?: PlayerStatus;
  /** 'napo' = ナポ陣営、'army' = 連合軍。avatar の枠色と王冠マークに反映。 */
  team?: Team;
  /** active = ターン中。avatar に halo を出して pulse させる。 */
  active?: boolean;
  /** PC seat 用のコンパクト + 横並び。 */
  compact?: boolean;
  horizontal?: boolean;
  align?: "left" | "right";
  /** ナポレオン宣言文(例: "13♠")。あれば status より優先表示。 */
  bid?: string;
  /** 累積得点。表示すると + が緑、- が赤。 */
  total?: number;
  /** Napoleon 本人(王冠 SVG)。 */
  isNapo?: boolean;
  /** 副官(発覚済み)。"副" バッジ。 */
  isPartner?: boolean;
  /** 切断中。avatar 灰色化 + ラベル表示。 */
  disconnected?: boolean;
  /** 切断時刻 (ms epoch)。`disconnected` が true のとき経過秒を表示。 */
  disconnectedSinceMs?: number;
}

export function PlayerBadge(props: PlayerBadgeProps) {
  const {
    name,
    status = "",
    team = null,
    active = false,
    compact,
    horizontal,
    align = "left",
    bid,
    total,
    isNapo,
    isPartner,
    disconnected = false,
    disconnectedSinceMs,
  } = props;
  const statusText = STATUS_TEXT[status];

  // PC corner seat 用の横並びレイアウト。
  if (compact && horizontal) {
    const flexDir = align === "right" ? "row-reverse" : "row";
    const totalColor =
      total !== undefined && total > 0
        ? "var(--accent-success)"
        : total !== undefined && total < 0
          ? "var(--accent-primary)"
          : "var(--text-muted)";
    return (
      <div
        style={{
          display: "flex",
          flexDirection: flexDir,
          alignItems: "center",
          gap: 8,
          opacity: status === "pass" ? 0.45 : 1,
          transition: "opacity 200ms",
        }}
      >
        <div style={{ position: "relative" }}>
          <Avatar name={name} size={36} active={active} disconnected={disconnected} />
          {isNapo && <NapoleonCrown />}
          {team === "napo" && !isNapo && <NapoChip />}
          {isPartner && <PartnerChip />}
        </div>
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: align === "right" ? "flex-end" : "flex-start",
            gap: 1,
            minWidth: 64,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "baseline",
              gap: 6,
              flexDirection: align === "right" ? "row-reverse" : "row",
            }}
          >
            <span
              style={{
                fontSize: 13,
                color: "var(--text-primary)",
                fontWeight: 600,
                letterSpacing: 0.2,
              }}
            >
              {name}
            </span>
            {total !== undefined && (
              <span
                style={{
                  fontSize: 12,
                  fontFamily: "var(--font-serif)",
                  color: totalColor,
                  fontWeight: 600,
                  background: "var(--bg-secondary)",
                  padding: "0 5px",
                  borderRadius: 3,
                  lineHeight: 1.4,
                }}
              >
                {total > 0 ? "+" : ""}
                {total}
              </span>
            )}
          </div>
          {disconnected && disconnectedSinceMs !== undefined ? (
            <DisconnectedBadge sinceMs={disconnectedSinceMs} />
          ) : (
            <span
              style={{
                fontSize: 10,
                color: "var(--text-secondary)",
                fontFamily: "var(--font-serif)",
              }}
            >
              {bid ? bid : statusText}
            </span>
          )}
        </div>
      </div>
    );
  }

  // モバイル用のコンパクト縦積み。
  if (compact) {
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 4,
          opacity: status === "pass" ? 0.45 : 1,
          transition: "opacity 200ms",
        }}
      >
        <div style={{ position: "relative" }}>
          <Avatar name={name} size={36} team={team} active={active} disconnected={disconnected} />
          {team === "napo" && <NapoChip />}
        </div>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2 }}>
          <span
            style={{
              fontSize: 13,
              color: "var(--text-primary)",
              fontWeight: 600,
              letterSpacing: 0.2,
            }}
          >
            {name}
          </span>
          {disconnected && disconnectedSinceMs !== undefined ? (
            <DisconnectedBadge sinceMs={disconnectedSinceMs} />
          ) : (
            <span
              style={{
                fontSize: 10,
                color: "var(--text-secondary)",
                fontFamily: "var(--font-serif)",
              }}
            >
              {bid ? bid : statusText}
            </span>
          )}
        </div>
      </div>
    );
  }

  // フルサイズ(検証 UI / モーダル等)。
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 6,
        padding: "8px 10px",
        borderRadius: "var(--radius-md)",
        background: active ? "rgba(217,83,79,0.06)" : "transparent",
        border: active ? "1px dashed rgba(217,83,79,0.4)" : "1px solid transparent",
        transition: "all 200ms",
        opacity: status === "pass" ? 0.5 : 1,
        minWidth: 88,
      }}
    >
      <div style={{ position: "relative" }}>
        <Avatar name={name} size={48} team={team} active={active} />
        {team === "napo" && <NapoChip large />}
      </div>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2 }}>
        <span style={{ fontSize: 13, fontWeight: 600, color: "var(--text-primary)" }}>{name}</span>
        {bid ? (
          <span
            style={{
              fontSize: 12,
              color: "var(--accent-primary)",
              fontWeight: 600,
              fontFamily: "var(--font-serif)",
            }}
          >
            {bid}
          </span>
        ) : (
          <span
            style={{
              fontSize: 11,
              color: status === "played" ? "var(--accent-success)" : "var(--text-secondary)",
              fontWeight: status === "played" ? 600 : 400,
            }}
          >
            {statusText}
          </span>
        )}
      </div>
    </div>
  );
}

/** Napoleon 本人の頭上に乗る金色の王冠 SVG。 */
function NapoleonCrown() {
  return (
    <div
      style={{
        position: "absolute",
        top: -10,
        left: "50%",
        transform: "translateX(-50%)",
        filter: "drop-shadow(0 1px 1px rgba(0,0,0,0.25))",
      }}
    >
      <svg width="22" height="14" viewBox="0 0 22 16" style={{ overflow: "visible" }}>
        <path
          d="M2 14 L4 4 L8 9 L11 2 L14 9 L18 4 L20 14 Z"
          fill="url(#crownGradPB)"
          stroke="#8a5e10"
          strokeWidth="0.8"
          strokeLinejoin="round"
        />
        <circle cx="4" cy="4" r="1.4" fill="#d99a2a" stroke="#8a5e10" strokeWidth="0.5" />
        <circle cx="11" cy="2" r="1.4" fill="#d99a2a" stroke="#8a5e10" strokeWidth="0.5" />
        <circle cx="18" cy="4" r="1.4" fill="#d99a2a" stroke="#8a5e10" strokeWidth="0.5" />
        <line x1="2" y1="14" x2="20" y2="14" stroke="#8a5e10" strokeWidth="1" />
        <defs>
          <linearGradient id="crownGradPB" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#f4d96b" />
            <stop offset="100%" stopColor="#d99a2a" />
          </linearGradient>
        </defs>
      </svg>
    </div>
  );
}

/** team='napo' な avatar に付ける小さな王冠記号。 */
function NapoChip({ large = false }: { large?: boolean }) {
  return (
    <div
      style={{
        position: "absolute",
        top: large ? -8 : -6,
        right: large ? -6 : -4,
        fontSize: large ? 16 : 13,
        filter: "drop-shadow(0 1px 1px rgba(0,0,0,0.3))",
      }}
    >
      ♛
    </div>
  );
}

/** 副官バッジ(発覚後)。 */
function PartnerChip() {
  return (
    <div
      style={{
        position: "absolute",
        top: -6,
        right: -6,
        width: 18,
        height: 18,
        borderRadius: "50%",
        background: "var(--accent-primary)",
        color: "#fff",
        fontSize: 11,
        fontWeight: 700,
        fontFamily: "var(--font-sans)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        border: "2px solid var(--bg-primary)",
        boxShadow: "0 1px 3px rgba(0,0,0,0.25)",
        letterSpacing: 0,
      }}
    >
      副
    </div>
  );
}
