// Phase B-3 / C 検証用 UI。store + WS の動作 + Card / Player ギャラリーを表示。
// 本物のゲーム画面は Phase D で差し替え。

import { useState } from "react";
import { TrumpCard } from "./components/Card";
import { Avatar, PlayerBadge } from "./components/Player";
import { useStore } from "./lib/store";

function App() {
  const status = useStore((s) => s.status);
  const myEmail = useStore((s) => s.myEmail);
  const myHandle = useStore((s) => s.myHandle);
  const serverVersion = useStore((s) => s.serverVersion);
  const room = useStore((s) => s.room);
  const events = useStore((s) => s.events);
  const errors = useStore((s) => s.errors);
  const connect = useStore((s) => s.connect);
  const send = useStore((s) => s.send);
  const disconnect = useStore((s) => s.disconnect);

  const [displayName, setDisplayName] = useState("alice");
  const [devEmail, setDevEmail] = useState("alice@local");
  const [roomIdInput, setRoomIdInput] = useState("");

  return (
    <div
      style={{
        padding: "2rem",
        maxWidth: 720,
        margin: "0 auto",
        fontFamily: "var(--font-sans)",
      }}
    >
      <h1
        style={{
          fontFamily: "var(--font-serif)",
          fontSize: "2rem",
          margin: 0,
          marginBottom: "0.5rem",
        }}
      >
        napo-online
      </h1>
      <p style={{ color: "var(--text-secondary)", margin: "0 0 1.5rem" }}>
        Phase B-3: WebSocket + Zustand 検証。Phase D で本物のゲーム画面に差し替え。
      </p>

      <section style={panelStyle}>
        <h2 style={h2Style}>接続</h2>
        <div style={rowStyle}>
          <span style={badgeStyle(status)}>{status}</span>
          {myEmail && <span style={{ color: "var(--text-secondary)" }}>{myEmail}</span>}
          {myHandle && <span>handle: {myHandle}</span>}
          {serverVersion && (
            <span style={{ color: "var(--text-muted)" }}>server v{serverVersion}</span>
          )}
        </div>
        {status === "disconnected" && (
          <div style={{ ...rowStyle, marginTop: "0.75rem" }}>
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="表示名"
              style={inputStyle}
            />
            <input
              value={devEmail}
              onChange={(e) => setDevEmail(e.target.value)}
              placeholder="dev_email"
              style={inputStyle}
            />
            <button onClick={() => connect(displayName, devEmail)} style={btnStyle}>
              接続
            </button>
          </div>
        )}
        {status === "connected" && (
          <div style={{ ...rowStyle, marginTop: "0.75rem" }}>
            <button onClick={disconnect} style={btnStyle}>
              切断
            </button>
            <button onClick={() => send({ type: "create_room" })} style={btnStyle}>
              CreateRoom
            </button>
            <input
              value={roomIdInput}
              onChange={(e) => setRoomIdInput(e.target.value)}
              placeholder="room_id"
              style={inputStyle}
            />
            <button
              onClick={() => send({ type: "join_room", payload: { room_id: roomIdInput } })}
              style={btnStyle}
            >
              JoinRoom
            </button>
          </div>
        )}
      </section>

      {room && (
        <section style={panelStyle}>
          <h2 style={h2Style}>部屋: {room.room_id}</h2>
          <p style={{ color: "var(--text-secondary)", margin: 0 }}>
            host: {room.host} / state: {room.state.type}
          </p>
          <ul style={{ paddingLeft: "1.2rem", marginTop: "0.5rem" }}>
            {room.players.map((p) => (
              <li key={p.player_id} style={{ color: p.online ? undefined : "var(--text-muted)" }}>
                {p.display_name} ({p.player_id}) {p.seat !== null && `seat=${p.seat}`}{" "}
                {!p.online && "— offline"}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section style={panelStyle}>
        <h2 style={h2Style}>イベントログ ({events.length})</h2>
        <pre
          style={{
            fontSize: "0.8rem",
            background: "var(--bg-secondary)",
            padding: "0.5rem",
            borderRadius: "var(--radius-sm)",
            maxHeight: "12rem",
            overflowY: "auto",
            margin: 0,
          }}
        >
          {events.map((e, i) => `${i}: ${JSON.stringify(e)}`).join("\n")}
        </pre>
      </section>

      {errors.length > 0 && (
        <section style={panelStyle}>
          <h2 style={{ ...h2Style, color: "var(--accent-primary)" }}>エラー</h2>
          <ul style={{ paddingLeft: "1.2rem", margin: 0 }}>
            {errors.slice(-5).map((e, i) => (
              <li key={i} style={{ color: "var(--accent-primary)" }}>
                [{e.code}] {e.message}
              </li>
            ))}
          </ul>
        </section>
      )}

      <ComponentGallery />
    </div>
  );
}

/** Phase C で移植した Card / Player を視覚チェックするためのギャラリー。
 * Phase D 開始時に削除予定。 */
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
          <PlayerBadge name="ハル" status="thinking" count={7} active />
          <PlayerBadge name="ナギ" status="played" count={6} team="napo" isNapo bid="13♠" />
          <PlayerBadge name="リオ" status="pass" count={7} team="army" />
          <PlayerBadge name="ミナト" count={5} team="napo" isPartner />
          <PlayerBadge name="アオイ" status="waiting" count={7} total={-3} />
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
          <PlayerBadge name="ナギ" compact horizontal align="right" total={-2} count={6} />
        </div>
      </section>
    </>
  );
}

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
