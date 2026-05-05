// tmp/design/overlays.jsx を TSX 化。phase ごとに full-screen overlay を表示する。
// - bid: 入札ラウンド中。各 seat の現在 bid を表示 + 自分の bid 入力
// - partner: 副官指名中(declarer が選択するのを待つ全員向け表示)
// - trick-result: 1トリック完了で勝者を発表
// - end: ゲーム(局)終了 + standings
//
// shared 定数(overlayBase / pillBtn / primaryBtn など)は Overlays.tsx 内に閉じる。

import type { CSSProperties } from "react";
import type { SuitKey } from "./card-shared";
import { Suit, TrumpCard } from "./Card";
import { Avatar } from "./Player";

export type OverlayPhase = "bid" | "partner" | "trick-result" | "end" | null;

export type CornerPos = "tl" | "tr" | "bl" | "br";

// ====================== Bid ======================

export interface BidSeatVM {
  name: string;
  pos: CornerPos;
  /** 現状の宣言テキスト("13 ♠" や "パス")、または "—"(まだ未行動)。 */
  bid: string;
  passed: boolean;
  /** 現在の最高 bidder。highlight 表示。 */
  leading: boolean;
  /** "考え中…"を出す時。 */
  waiting: boolean;
}

export interface BidOverlayProps {
  mobile?: boolean;
  /** 4 opponent seats(自分は中央下に固定で表示)。 */
  opponents: BidSeatVM[];
  /** 自分のハンドル名。 */
  myName: string;
  /** ヘッダーに出す「現在の最高ビッド」。null なら「まだ無し」表示。 */
  currentTopBid: { tricks: number; trump: SuitKey | "NT"; by: string } | null;
  /** 自分のターンなら入力 UI を表示。 */
  myTurn: boolean;
  /** 入札可能な最低 trick 数。 */
  minTricks: number;
  /** 選択中の trick 数(コントロール側で持つ)。 */
  selectedTricks: number;
  /** 選択中のスート。 */
  selectedTrump: SuitKey | "NT";
  onSelectTricks: (n: number) => void;
  onSelectTrump: (s: SuitKey | "NT") => void;
  onPass: () => void;
  onBid: () => void;
}

export function BidOverlay({
  mobile,
  opponents,
  myName,
  currentTopBid,
  myTurn,
  minTricks,
  selectedTricks,
  selectedTrump,
  onSelectTricks,
  onSelectTrump,
  onPass,
  onBid,
}: BidOverlayProps) {
  const tickRange = [minTricks, minTricks + 1, minTricks + 2, minTricks + 3];
  return (
    <div style={overlayBase}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background:
            "radial-gradient(ellipse at center, rgba(232,223,200,0.95) 0%, rgba(251,247,238,0.98) 80%)",
          backdropFilter: "blur(3px)",
        }}
      />

      <div
        style={{
          position: "absolute",
          top: mobile ? 12 : 24,
          left: 0,
          right: 0,
          textAlign: "center",
          zIndex: 2,
        }}
      >
        <div
          style={{
            fontSize: mobile ? 10 : 12,
            letterSpacing: 4,
            color: "var(--text-muted)",
            marginBottom: 4,
          }}
        >
          BIDDING
        </div>
        <h1
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: mobile ? 22 : 32,
            margin: "0 0 4px",
            color: "var(--text-primary)",
            letterSpacing: -0.5,
          }}
        >
          立ち合い
        </h1>
        <p style={{ fontSize: mobile ? 10 : 12, color: "var(--text-secondary)", margin: 0 }}>
          {currentTopBid ? (
            <>
              現在の最高ビッド &nbsp;
              <span
                style={{
                  fontFamily: "var(--font-serif)",
                  color: "var(--accent-primary)",
                  fontWeight: 600,
                  fontSize: mobile ? 14 : 16,
                }}
              >
                {currentTopBid.tricks}
              </span>
              {currentTopBid.trump === "NT" ? (
                <span style={{ marginLeft: 4 }}>NT</span>
              ) : (
                <Suit s={currentTopBid.trump} size={mobile ? 12 : 14} />
              )}
              &nbsp;by <span style={{ fontWeight: 600 }}>{currentTopBid.by}</span>
            </>
          ) : (
            "宣言待ち"
          )}
        </p>
      </div>

      <div
        style={{
          position: "absolute",
          inset: mobile ? "60px 8px 90px" : "90px 22px 22px",
          zIndex: 2,
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
          {opponents.map((s) => (
            <BidSeat key={s.name} {...s} mobile={mobile} />
          ))}

          {myTurn && (
            <div
              style={{
                position: "absolute",
                left: "50%",
                top: "50%",
                transform: "translate(-50%, -50%)",
                zIndex: 3,
                textAlign: "center",
              }}
            >
              <div
                style={{
                  fontSize: mobile ? 10 : 11,
                  color: "var(--text-muted)",
                  letterSpacing: 2,
                  marginBottom: 8,
                }}
              >
                あなたの番
              </div>
              <div
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: mobile ? 6 : 10,
                  padding: mobile ? "8px 12px" : "12px 18px",
                  background: "var(--bg-primary)",
                  borderRadius: "var(--radius-md)",
                  border: "2px solid var(--accent-primary)",
                  boxShadow: "0 6px 18px rgba(217,83,79,0.22)",
                  flexWrap: "wrap",
                  justifyContent: "center",
                }}
              >
                <span style={{ fontSize: mobile ? 10 : 12, color: "var(--text-secondary)" }}>
                  枚数
                </span>
                {tickRange.map((n) => (
                  <button
                    type="button"
                    key={n}
                    onClick={() => onSelectTricks(n)}
                    style={{
                      ...pillBtn,
                      padding: mobile ? "4px 8px" : "6px 12px",
                      fontSize: mobile ? 12 : 14,
                      background: n === selectedTricks ? "var(--accent-primary)" : "transparent",
                      color: n === selectedTricks ? "#fff" : "var(--text-primary)",
                      fontFamily: "var(--font-serif)",
                    }}
                  >
                    {n}
                  </button>
                ))}
                <Sep />
                {(["S", "H", "D", "C", "NT"] as const).map((s) => (
                  <button
                    type="button"
                    key={s}
                    onClick={() => onSelectTrump(s)}
                    style={{
                      ...pillBtn,
                      padding: mobile ? "4px 6px" : "6px 8px",
                      background: s === selectedTrump ? "var(--bg-secondary)" : "transparent",
                      borderRadius: 4,
                    }}
                  >
                    {s === "NT" ? (
                      <span style={{ fontSize: mobile ? 11 : 13 }}>NT</span>
                    ) : (
                      <Suit s={s} size={mobile ? 14 : 18} />
                    )}
                  </button>
                ))}
                <Sep />
                <button
                  type="button"
                  onClick={onPass}
                  style={{
                    ...pillBtn,
                    padding: mobile ? "4px 10px" : "6px 14px",
                    fontSize: mobile ? 11 : 13,
                    color: "var(--text-secondary)",
                    border: "1px solid var(--border-color)",
                  }}
                >
                  パス
                </button>
                <button
                  type="button"
                  onClick={onBid}
                  style={{
                    ...primaryBtn,
                    padding: mobile ? "5px 10px" : "6px 16px",
                    fontSize: mobile ? 11 : 13,
                  }}
                >
                  宣言
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      <div
        style={{
          position: "absolute",
          left: "50%",
          bottom: mobile ? 16 : 28,
          transform: "translateX(-50%)",
          zIndex: 2,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 6,
        }}
      >
        <Avatar name={myName} size={mobile ? 40 : 56} active={myTurn} />
        <div style={{ fontSize: mobile ? 11 : 13, fontWeight: 600 }}>
          {myName}{" "}
          <span style={{ fontSize: mobile ? 9 : 10, color: "var(--text-muted)", fontWeight: 400 }}>
            (あなた)
          </span>
        </div>
      </div>
    </div>
  );
}

function BidSeat({
  name,
  pos,
  bid,
  passed,
  leading,
  waiting,
  mobile,
}: BidSeatVM & { mobile?: boolean }) {
  const isTop = pos === "tl" || pos === "tr";
  const isLeft = pos === "tl" || pos === "bl";
  const inset = isTop ? "18%" : 0;
  const avSize = mobile ? 36 : 52;
  const containerStyle: CSSProperties = {
    position: "absolute",
    [isTop ? "top" : "bottom"]: 0,
    [isLeft ? "left" : "right"]: inset,
    display: "flex",
    flexDirection: "column",
    alignItems: isLeft ? "flex-start" : "flex-end",
    gap: 6,
    opacity: passed ? 0.45 : 1,
  };
  return (
    <div style={containerStyle}>
      <div
        style={{
          display: "flex",
          flexDirection: isLeft ? "row" : "row-reverse",
          alignItems: "center",
          gap: 8,
        }}
      >
        <div style={{ position: "relative" }}>
          <Avatar name={name} size={avSize} />
          {leading && (
            <div
              style={{
                position: "absolute",
                top: -10,
                left: "50%",
                transform: "translateX(-50%)",
                fontSize: mobile ? 14 : 18,
                filter: "drop-shadow(0 1px 2px rgba(201,162,76,0.6))",
              }}
            >
              ♛
            </div>
          )}
        </div>
        <div style={{ fontSize: mobile ? 11 : 13, fontWeight: 600 }}>{name}</div>
      </div>
      <div
        style={{
          position: "relative",
          background: leading ? "var(--accent-primary)" : passed ? "var(--bg-secondary)" : "#fff",
          color: leading ? "#fff" : passed ? "var(--text-muted)" : "var(--text-primary)",
          padding: mobile ? "4px 10px" : "6px 14px",
          borderRadius: 12,
          fontSize: mobile ? 11 : 13,
          fontFamily: "var(--font-serif)",
          fontWeight: leading ? 600 : 400,
          border: passed
            ? "1px solid var(--border-color)"
            : leading
              ? "none"
              : "1px solid var(--border-color)",
          boxShadow: leading ? "0 2px 6px rgba(217,83,79,0.3)" : "0 1px 2px rgba(0,0,0,0.05)",
          whiteSpace: "nowrap",
          fontStyle: waiting ? "italic" : "normal",
          marginLeft: isLeft ? avSize + 4 : 0,
          marginRight: isLeft ? 0 : avSize + 4,
        }}
      >
        {waiting ? "考え中…" : bid}
      </div>
    </div>
  );
}

// ====================== Partner (副官指名待ち) ======================

export interface PartnerOverlayProps {
  mobile?: boolean;
  /** declarer の名前(「ミナトが副官カードを選んでいます」)。 */
  declarerName: string;
}

export function PartnerOverlay({ mobile, declarerName }: PartnerOverlayProps) {
  return (
    <div style={overlayBase}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: "rgba(43,38,32,0.7)",
          backdropFilter: "blur(6px)",
        }}
      />
      <div
        style={{
          position: "relative",
          background: "var(--bg-primary)",
          borderRadius: "var(--radius-lg)",
          padding: mobile ? 20 : 36,
          boxShadow: "var(--shadow-lg)",
          maxWidth: mobile ? 340 : 620,
          width: "90%",
          textAlign: "center",
        }}
      >
        <div
          style={{
            fontSize: mobile ? 10 : 12,
            letterSpacing: 4,
            color: "var(--text-muted)",
            marginBottom: 4,
          }}
        >
          SECRET PARTNER
        </div>
        <h1
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: mobile ? 22 : 30,
            margin: "0 0 8px",
            color: "var(--text-primary)",
          }}
        >
          {declarerName}が副官カードを選んでいます
        </h1>
        <p
          style={{ fontSize: mobile ? 11 : 13, color: "var(--text-secondary)", margin: "0 0 22px" }}
        >
          選ばれたカードを持つ人が{declarerName}の味方となります(全員に秘密)
        </p>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: mobile ? 6 : 10,
            alignItems: "center",
            opacity: 0.85,
          }}
        >
          {(["S", "H", "D", "C"] as const).map((s) => (
            <div key={s} style={{ display: "flex", gap: mobile ? 2 : 4 }}>
              {(["A", "K", "Q", "J", "10", "9", "8", "7", "6", "5", "4", "3", "2"] as const).map(
                (r) => (
                  <div key={r} style={{ filter: "grayscale(0.5) opacity(0.65)" }}>
                    <TrumpCard code={s + r} w={mobile ? 22 : 36} h={mobile ? 32 : 52} />
                  </div>
                ),
              )}
            </div>
          ))}
        </div>

        <div
          style={{
            marginTop: mobile ? 16 : 24,
            display: "flex",
            justifyContent: "center",
            gap: 10,
            alignItems: "center",
          }}
        >
          <div
            style={{
              padding: mobile ? "5px 14px" : "7px 18px",
              background: "var(--bg-secondary)",
              borderRadius: 99,
              fontSize: mobile ? 11 : 13,
              color: "var(--text-secondary)",
              display: "inline-flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: "var(--accent-primary)",
                animation: "turn-pulse 1.5s ease-in-out infinite",
              }}
            />
            {declarerName}の選択を待っています…
          </div>
        </div>
      </div>
    </div>
  );
}

// ====================== TrickResult ======================

export interface TrickResultOverlayProps {
  mobile?: boolean;
  trickIndex: number;
  winnerName: string;
  /** その trick で勝った絵札枚数(face cards captured)。 */
  facesGained: number;
  /** 場の 4(or 5)枚を勝者の手元に集めるアニメ用。 */
  cards: { card: string; rot: number }[];
}

export function TrickResultOverlay({
  mobile,
  trickIndex,
  winnerName,
  facesGained,
  cards,
}: TrickResultOverlayProps) {
  return (
    <div style={overlayBase}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background:
            "radial-gradient(ellipse at center, rgba(122,139,92,0.18), rgba(43,38,32,0.55))",
          backdropFilter: "blur(2px)",
        }}
      />
      <div style={{ position: "relative", textAlign: "center", color: "#fff" }}>
        <div
          style={{ fontSize: mobile ? 10 : 12, letterSpacing: 4, opacity: 0.7, marginBottom: 6 }}
        >
          TRICK {trickIndex}
        </div>
        <h1
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: mobile ? 26 : 42,
            margin: "0 0 16px",
            letterSpacing: -0.5,
            textShadow: "0 4px 20px rgba(0,0,0,0.4)",
          }}
        >
          {winnerName} さんが取りました
        </h1>
        <div
          style={{
            position: "relative",
            width: mobile ? 220 : 340,
            height: mobile ? 130 : 180,
            margin: "0 auto 16px",
          }}
        >
          {cards.map((p, i) => (
            <div
              key={i}
              style={{
                position: "absolute",
                left: mobile ? (60 + i * 40) * 0.8 : 60 + i * 40,
                top: mobile ? 30 : 40,
                animation: `gather 600ms var(--ease-default) ${i * 50}ms backwards`,
              }}
            >
              <TrumpCard code={p.card} w={mobile ? 50 : 68} h={mobile ? 74 : 100} rot={p.rot} />
            </div>
          ))}
        </div>
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 12,
            padding: mobile ? "8px 16px" : "12px 24px",
            background: "rgba(251,247,238,0.95)",
            color: "var(--text-primary)",
            borderRadius: 99,
            boxShadow: "0 8px 24px rgba(0,0,0,0.3)",
          }}
        >
          <Avatar name={winnerName} size={mobile ? 28 : 36} />
          <div style={{ textAlign: "left" }}>
            <div style={{ fontSize: mobile ? 11 : 13, fontWeight: 600 }}>{winnerName}</div>
            <div
              style={{
                fontSize: mobile ? 10 : 12,
                color: "var(--accent-success)",
                fontWeight: 600,
              }}
            >
              {facesGained > 0 ? `+${facesGained} 絵札` : "絵札 0"}
            </div>
          </div>
          <div style={{ width: 1, height: 24, background: "var(--border-color)" }} />
          <div style={{ fontSize: mobile ? 10 : 12, color: "var(--text-secondary)" }}>
            次のリーダー:{" "}
            <span style={{ fontWeight: 600, color: "var(--text-primary)" }}>{winnerName}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

// ====================== End (game/match end) ======================

export interface TeamPlayerVM {
  name: string;
  tricks: number;
  picture: number;
  hidden?: "partner";
}

export interface StandingVM {
  name: string;
  total: number;
  delta: number;
  role: "napo" | "partner" | "army";
}

export interface EndOverlayProps {
  mobile?: boolean;
  napoTeam: TeamPlayerVM[];
  armyTeam: TeamPlayerVM[];
  napoScore: number;
  armyScore: number;
  /** napo 勝ちなら true、army 勝ちなら false。 */
  napoWon: boolean;
  /** 宣言値と実際取得した枚数。 */
  bidValue: number;
  taken: number;
  standings: StandingVM[];
  currentGame: number;
  totalGames: number;
  onNext?: () => void;
  /** Match 自体が完了した時(全 totalGames 終了 or EndMatch)、Next ボタンを隠す。 */
  matchEnded?: boolean;
}

export function EndOverlay({
  mobile,
  napoTeam,
  armyTeam,
  napoScore,
  armyScore,
  napoWon,
  bidValue,
  taken,
  standings,
  currentGame,
  totalGames,
  onNext,
  matchEnded,
}: EndOverlayProps) {
  const sorted = [...standings].sort((a, b) => b.total - a.total);
  return (
    <div style={overlayBase}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          background:
            "linear-gradient(180deg, rgba(217,83,79,0.18) 0%, rgba(251,247,238,0.98) 60%)",
          backdropFilter: "blur(4px)",
        }}
      />
      <Confetti />
      <div
        style={{
          position: "relative",
          textAlign: "center",
          padding: mobile ? 16 : 24,
          maxWidth: mobile ? 360 : 760,
          width: "92%",
          maxHeight: "92%",
          overflowY: "auto",
        }}
      >
        <div
          style={{
            display: "inline-flex",
            alignItems: "baseline",
            gap: 8,
            padding: mobile ? "3px 10px" : "4px 14px",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border-color)",
            borderRadius: 99,
            marginBottom: mobile ? 8 : 12,
            fontSize: mobile ? 10 : 12,
            color: "var(--text-secondary)",
            letterSpacing: 1,
          }}
        >
          <span>GAME</span>
          <span
            style={{
              fontFamily: "var(--font-serif)",
              fontSize: mobile ? 14 : 17,
              color: "var(--text-primary)",
              fontWeight: 600,
            }}
          >
            {currentGame}
          </span>
          <span style={{ color: "var(--text-muted)" }}>/</span>
          <span
            style={{
              fontFamily: "var(--font-serif)",
              fontSize: mobile ? 12 : 14,
              color: "var(--text-muted)",
            }}
          >
            {totalGames}
          </span>
          <span style={{ marginLeft: 4, fontSize: mobile ? 9 : 10, color: "var(--text-muted)" }}>
            {matchEnded ? "セット終了" : "終了"}
          </span>
        </div>
        <h1
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: mobile ? 28 : 46,
            margin: "0 0 4px",
            color: napoWon ? "var(--accent-primary)" : "var(--accent-secondary)",
            letterSpacing: -1,
            textShadow: "0 2px 0 rgba(217,83,79,0.1)",
          }}
        >
          {napoWon ? "ナポレオン軍 勝利" : "連合軍 勝利"}
        </h1>
        <p
          style={{ fontSize: mobile ? 12 : 14, color: "var(--text-secondary)", margin: "0 0 18px" }}
        >
          宣言 <span style={{ fontFamily: "var(--font-serif)" }}>{bidValue}</span> に対して
          <span
            style={{
              fontFamily: "var(--font-serif)",
              color: "var(--accent-primary)",
              fontWeight: 600,
            }}
          >
            {" "}
            {taken}
          </span>{" "}
          枚獲得
        </p>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: mobile ? "1fr" : "1fr 40px 1fr",
            gap: mobile ? 10 : 0,
            alignItems: "stretch",
            marginBottom: mobile ? 14 : 20,
          }}
        >
          <TeamCard
            team="napo"
            label="ナポレオン軍"
            players={napoTeam}
            score={napoScore}
            won={napoWon}
            mobile={mobile}
          />
          {!mobile && (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontFamily: "var(--font-serif)",
                fontSize: 22,
                color: "var(--text-muted)",
              }}
            >
              vs
            </div>
          )}
          <TeamCard
            team="army"
            label="連合軍"
            players={armyTeam}
            score={armyScore}
            won={!napoWon}
            mobile={mobile}
          />
        </div>

        <Standings
          sorted={sorted}
          mobile={mobile}
          currentGame={currentGame}
          totalGames={totalGames}
        />

        {!matchEnded && onNext && (
          <div
            style={{
              display: "flex",
              gap: 10,
              justifyContent: "center",
              marginTop: mobile ? 16 : 22,
              flexWrap: "wrap",
            }}
          >
            <button
              type="button"
              onClick={onNext}
              style={{
                ...primaryBtn,
                padding: mobile ? "10px 22px" : "12px 28px",
                fontSize: mobile ? 13 : 15,
              }}
            >
              次のゲームへ &nbsp;
              <span
                style={{
                  opacity: 0.7,
                  fontFamily: "var(--font-serif)",
                  fontSize: mobile ? 12 : 14,
                }}
              >
                {currentGame + 1} / {totalGames}
              </span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function Standings({
  sorted,
  mobile,
  currentGame,
  totalGames,
}: {
  sorted: StandingVM[];
  mobile?: boolean;
  currentGame: number;
  totalGames: number;
}) {
  const max = Math.max(...sorted.map((s) => Math.abs(s.total)), 1);
  return (
    <div
      style={{
        background: "var(--bg-primary)",
        border: "1px solid var(--border-color)",
        borderRadius: "var(--radius-md)",
        padding: mobile ? 12 : 16,
        textAlign: "left",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          marginBottom: mobile ? 8 : 12,
          paddingBottom: 6,
          borderBottom: "1px solid var(--border-color)",
        }}
      >
        <div style={{ display: "flex", alignItems: "baseline", gap: 8 }}>
          <span
            style={{ fontSize: mobile ? 12 : 14, fontWeight: 600, color: "var(--text-primary)" }}
          >
            セット総合得点
          </span>
          <span
            style={{ fontSize: mobile ? 10 : 11, color: "var(--text-muted)", letterSpacing: 0.5 }}
          >
            ゲーム <span style={{ fontFamily: "var(--font-serif)" }}>{currentGame}</span> 時点 (残り{" "}
            <span style={{ fontFamily: "var(--font-serif)" }}>{totalGames - currentGame}</span>)
          </span>
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: mobile ? 6 : 8 }}>
        {sorted.map((s, idx) => {
          const positive = s.total >= 0;
          const barWidth = (Math.abs(s.total) / max) * 100;
          const roleColor =
            s.role === "napo" || s.role === "partner"
              ? "var(--accent-primary)"
              : "var(--accent-secondary)";
          const roleLabel = s.role === "napo" ? "ナポ" : s.role === "partner" ? "副官" : "連合";
          return (
            <div
              key={s.name}
              style={{
                display: "grid",
                gridTemplateColumns: mobile ? "20px 1fr 60px 50px" : "24px 1fr 1fr 80px 60px",
                alignItems: "center",
                gap: mobile ? 8 : 12,
              }}
            >
              <span
                style={{
                  fontFamily: "var(--font-serif)",
                  fontSize: mobile ? 13 : 16,
                  color: idx === 0 ? "var(--accent-warning)" : "var(--text-muted)",
                  fontWeight: idx === 0 ? 700 : 400,
                  textAlign: "center",
                }}
              >
                {idx + 1}
              </span>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Avatar name={s.name} size={mobile ? 24 : 28} />
                <span style={{ fontSize: mobile ? 12 : 14, fontWeight: 600 }}>{s.name}</span>
                <span
                  style={{
                    fontSize: mobile ? 9 : 10,
                    padding: "1px 5px",
                    background:
                      s.role === "napo"
                        ? "rgba(217,83,79,0.14)"
                        : s.role === "partner"
                          ? "rgba(217,83,79,0.10)"
                          : "rgba(74,111,165,0.10)",
                    color: roleColor,
                    borderRadius: 3,
                    fontWeight: 500,
                  }}
                >
                  {roleLabel}
                </span>
              </div>
              {!mobile && (
                <div
                  style={{
                    position: "relative",
                    height: 6,
                    background: "var(--bg-secondary)",
                    borderRadius: 3,
                    overflow: "hidden",
                  }}
                >
                  <div
                    style={{
                      position: "absolute",
                      left: positive ? "50%" : `${50 - barWidth / 2}%`,
                      width: `${barWidth / 2}%`,
                      top: 0,
                      bottom: 0,
                      background: positive ? "var(--accent-success)" : "var(--accent-primary)",
                      opacity: 0.7,
                    }}
                  />
                  <div
                    style={{
                      position: "absolute",
                      left: "50%",
                      top: -2,
                      bottom: -2,
                      width: 1,
                      background: "var(--text-muted)",
                      opacity: 0.3,
                    }}
                  />
                </div>
              )}
              <span
                style={{
                  fontFamily: "var(--font-serif)",
                  fontSize: mobile ? 11 : 13,
                  color: s.delta > 0 ? "var(--accent-success)" : "var(--accent-primary)",
                  textAlign: "right",
                  fontWeight: 600,
                }}
              >
                {s.delta > 0 ? "+" : ""}
                {s.delta}
              </span>
              <span
                style={{
                  fontFamily: "var(--font-serif)",
                  fontSize: mobile ? 16 : 20,
                  color: positive ? "var(--text-primary)" : "var(--accent-primary)",
                  textAlign: "right",
                  fontWeight: 600,
                }}
              >
                {positive ? "+" : ""}
                {s.total}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TeamCard({
  team,
  label,
  players,
  score,
  won,
  mobile,
}: {
  team: "napo" | "army";
  label: string;
  players: TeamPlayerVM[];
  score: number;
  won: boolean;
  mobile?: boolean;
}) {
  const color = team === "napo" ? "var(--accent-primary)" : "var(--accent-secondary)";
  return (
    <div
      style={{
        background: "var(--bg-primary)",
        border: `2px solid ${won ? color : "var(--border-color)"}`,
        borderRadius: "var(--radius-md)",
        padding: mobile ? 12 : 18,
        boxShadow: won
          ? `0 6px 18px ${team === "napo" ? "rgba(217,83,79,0.2)" : "rgba(74,111,165,0.2)"}`
          : "none",
        opacity: won ? 1 : 0.85,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          marginBottom: 10,
          paddingBottom: 8,
          borderBottom: "1px solid var(--border-color)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ width: 8, height: 8, borderRadius: "50%", background: color }} />
          <span style={{ fontSize: mobile ? 12 : 14, fontWeight: 600 }}>{label}</span>
          {won && (
            <span
              style={{
                fontSize: mobile ? 9 : 10,
                padding: "1px 6px",
                background: color,
                color: "#fff",
                borderRadius: 99,
                letterSpacing: 0.5,
              }}
            >
              WIN
            </span>
          )}
        </div>
        <div
          style={{
            fontFamily: "var(--font-serif)",
            fontSize: mobile ? 22 : 28,
            color: "var(--text-primary)",
          }}
        >
          {score}
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {players.map((p) => (
          <div key={p.name} style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Avatar name={p.name} size={mobile ? 28 : 32} team={team} />
            <div style={{ flex: 1, textAlign: "left" }}>
              <div style={{ fontSize: mobile ? 11 : 13, fontWeight: 600 }}>
                {p.name}
                {p.hidden === "partner" && (
                  <span
                    style={{
                      fontSize: 10,
                      marginLeft: 6,
                      padding: "1px 5px",
                      background: "rgba(217,83,79,0.12)",
                      color: "var(--accent-primary)",
                      borderRadius: 3,
                      fontWeight: 500,
                    }}
                  >
                    副官
                  </span>
                )}
              </div>
              <div style={{ fontSize: mobile ? 10 : 11, color: "var(--text-muted)" }}>
                絵札 {p.picture} · トリック {p.tricks}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function Confetti() {
  const colors = ["#D9534F", "#4A6FA5", "#E0A93B", "#7A8B5C", "#D67843"];
  return (
    <div style={{ position: "absolute", inset: 0, overflow: "hidden", pointerEvents: "none" }}>
      {Array.from({ length: 28 }).map((_, i) => {
        const c = colors[i % colors.length];
        const left = (i * 53) % 100;
        const delay = (i * 137) % 1500;
        const dur = 2200 + ((i * 73) % 1500);
        const size = 5 + (i % 4);
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              left: `${left}%`,
              top: -10,
              width: size,
              height: size * 1.6,
              background: c,
              opacity: 0.85,
              animation: `confetti-fall ${dur}ms linear ${delay}ms infinite`,
              transform: `rotate(${i * 23}deg)`,
            }}
          />
        );
      })}
    </div>
  );
}

function Sep() {
  return <div style={{ width: 1, height: 18, background: "var(--border-color)" }} />;
}

const overlayBase: CSSProperties = {
  position: "absolute",
  inset: 0,
  zIndex: 50,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  fontFamily: "var(--font-sans)",
};

const pillBtn: CSSProperties = {
  background: "transparent",
  border: "none",
  borderRadius: 99,
  fontFamily: "var(--font-sans)",
  fontWeight: 500,
  cursor: "pointer",
};

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
