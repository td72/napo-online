# deploy/

VPS デプロイ用のサンプル設定。実環境で使うときは下記の変数を置換してください。

| ファイル | 役割 | 置換が必要な箇所 |
|---|---|---|
| `Caddyfile.example` | Caddy リバースプロキシ + HTTPS 終端 | `napo.<your-domain>` / `you@example.com` |
| `napo.service` | systemd ユニット | (必要に応じて User/Group/パス) |
| `napo.env.example` | 環境変数 | `NAPO_CF_TEAM` / `NAPO_CF_AUD` |

## 初回セットアップ(VPS 側、要 root)

```bash
# ユーザ作成
sudo useradd --system --create-home --shell /bin/false napo

# ディレクトリ
sudo mkdir -p /opt/napo /var/lib/napo /etc/napo
sudo chown napo:napo /opt/napo /var/lib/napo
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

# バイナリは別途 `just deploy`(ローカル側、§13.2)で送り込む
```

## 動作確認

```bash
sudo systemctl status napo
sudo journalctl -u napo -f
curl -s https://napo.<your-domain>/healthz   # 200 ならOK(CF Access が前段なので 401/403 になる場合あり)
```

## バックアップ

`/var/lib/napo/game.db` を定期的に `sqlite3 .backup` で保存:

```bash
# /etc/cron.daily/napo-backup
#!/bin/sh
mkdir -p /var/backups/napo
sqlite3 /var/lib/napo/game.db ".backup '/var/backups/napo/game-$(date +%Y%m%d).db'"
find /var/backups/napo -name 'game-*.db' -mtime +14 -delete
```
