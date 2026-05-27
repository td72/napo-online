# napo-online

5 人ナポレオン(Japanese Napoleon)のオンライン対戦サーバ + Web クライアント。

- Rust axum 0.8 + tokio + [napo-core](https://github.com/td72/napo-core) によるゲームサーバ
- React 19 + Vite+ + TypeScript の SPA クライアント
- WebSocket で局を進行、SQLite で room snapshot / chat / 履歴を永続化
- Cloudflare Access で OAuth、Caddy + Cloudflare Origin Cert で TLS

prod: <https://napo.td72.dev>(招待制)

## 開発

```sh
mise install
mise run dev:integrated   # Rust server + web bundle を local で起動
mise run ci               # rustfmt + clippy + test + web check 一式
```

詳細は [`docs/server-implementation-guide.md`](docs/server-implementation-guide.md) と [`deploy/README.md`](deploy/README.md) を参照。
