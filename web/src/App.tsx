// Phase A プレースホルダ。Phase D 以降で本物のゲーム画面に置き換え。
function App() {
  return (
    <div style={{ padding: '2rem', maxWidth: 720, margin: '0 auto' }}>
      <h1
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: '2rem',
          margin: 0,
          marginBottom: '0.5rem',
        }}
      >
        napo-online
      </h1>
      <p style={{ color: 'var(--text-secondary)' }}>
        React + Vite+ scaffold (Phase A)。/api と /ws は dev server から Rust(8080)へ proxy。
      </p>
      <p style={{ marginTop: '2rem', color: 'var(--text-muted)' }}>
        次の Phase: tokens フル抽出 + WebSocket クライアント + Zustand store + ts-rs 接続
      </p>
    </div>
  )
}

export default App
