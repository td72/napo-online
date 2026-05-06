# deploy/

VPS デプロイ用のサンプル設定。実環境で使うときは下記の変数を置換してください。

| ファイル | 役割 | 置換が必要な箇所 |
|---|---|---|
| `Caddyfile.example` | Caddy リバースプロキシ + HTTPS 終端 | `napo.<your-domain>` / `you@example.com` |
| `napo.service` | systemd ユニット | (必要に応じて User/Group/パス) |
| `napo.env.example` | 環境変数 | `NAPO_CF_TEAM` / `NAPO_CF_AUD` |
| `ansible/` | Ansible playbook(冪等な VPS プロビジョニング)| `deploy/ansible/README.md` を参照 |

**初回セットアップは Ansible 推奨。** 詳細は `deploy/ansible/README.md`。
手動でやりたい人向けに、以下に同等の bash 手順も残してある(両者は冪等性以外
等価)。

## 初回セットアップ(VPS 側、要 root)

```bash
# ユーザ作成
sudo useradd --system --create-home --shell /bin/false napo

# ディレクトリ(/opt/napo にバイナリと static/dist の両方を置く)
sudo mkdir -p /opt/napo /opt/napo/dist /var/lib/napo /etc/napo
sudo chown -R napo:napo /opt/napo /var/lib/napo
sudo chmod 750 /etc/napo

# 設定ファイル配置
sudo cp napo.env.example /etc/napo/napo.env
sudo $EDITOR /etc/napo/napo.env       # CF_TEAM/CF_AUD 等を入れる
sudo chown root:napo /etc/napo/napo.env
sudo chmod 640 /etc/napo/napo.env

# systemd 登録
sudo cp napo.service /etc/systemd/system/napo.service
sudo systemctl daemon-reload
sudo systemctl enable napo

# Caddy 設定
sudo cp Caddyfile.example /etc/caddy/Caddyfile
sudo $EDITOR /etc/caddy/Caddyfile
sudo systemctl reload caddy

# バイナリ + web bundle はローカル側から `mise run deploy` で送り込む
```

## 動作確認

```bash
sudo systemctl status napo
sudo journalctl -u napo -f
curl -s https://napo.<your-domain>/healthz   # 200 ならOK(CF Access が前段なので 401/403 になる場合あり)
```

## クロスコンパイル(ローカル → VPS)

### 前提(macOS の場合)

```bash
# 1. Docker(Docker Desktop / Rancher Desktop / OrbStack のいずれか)
brew install --cask docker        # 例

# 2. cross(Docker ベースの cargo cross-compile ラッパ)
cargo install cross

# 3. Rust target
rustup target add x86_64-unknown-linux-musl
```

`brew install FiloSottile/musl-cross/musl-cross` で musl-cross 直入れも可能だが、
GCC を source からビルドするので 30分〜1時間かかる。Docker が動くなら cross が圧倒的に速い。

### ビルド

```bash
mise run build:release
# → target/x86_64-unknown-linux-musl/release/napo-online (約 13MB, static-pie ELF)
```

実機検証済み: `cross build --release --target x86_64-unknown-linux-musl` で 4分前後。
出力は完全に static link なので glibc 依存無く Ubuntu 24.04 でそのまま動く。

### デプロイ

```bash
HOST=ubuntu@napo.example.com PORT=22 mise run deploy
```

`deploy` は次を一気に実行する:

1. `deploy:web`: `web:build` → `static/dist/` を rsync で `/opt/napo/dist/` に同期
2. `build:release`: musl static-pie バイナリを cross build
3. バイナリを scp → `/opt/napo/napo-online.new` に install → `.prev` に旧バイナリ退避 → `systemctl restart napo`

`NAPO_STATIC=/opt/napo/dist` が `napo.env` で設定されていれば、Rust 側が
React bundle (`/opt/napo/dist/index.html`) を fallback で配信する。

- `mise run deploy:web` … web bundle のみ転送(バイナリは更新しない)
- `mise run deploy:bin` … バイナリのみ転送(web 変更なし時)
- `mise run deploy:rollback` … `.prev` に戻す(バイナリのみ。web は別途巻き戻し)
- `mise run logs` … journalctl を tail
- `mise run health` … `/healthz` 疎通確認(`URL=https://...` で上書き可)
- `mise run db:pull` … 本番 game.db をローカルへ scp

## バックアップ

`/var/lib/napo/game.db` を定期的に `sqlite3 .backup` で保存:

```bash
# /etc/cron.daily/napo-backup
#!/bin/sh
mkdir -p /var/backups/napo
sqlite3 /var/lib/napo/game.db ".backup '/var/backups/napo/game-$(date +%Y%m%d).db'"
find /var/backups/napo -name 'game-*.db' -mtime +14 -delete
```
