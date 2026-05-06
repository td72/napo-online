# deploy/ansible/

VPS 初回プロビジョニング用 Ansible playbook。手順:
- system user / dirs
- `/etc/napo/napo.env` の生成(Cloudflare Access 等の値を inventory から)
- systemd unit 配置 + enable
- Caddyfile 配置 + reload

バイナリ + web bundle のデプロイは `mise run deploy`(ローカル → 本番に scp して
systemctl restart)が担当。Ansible はあくまで OS 上のお膳立て。

## 前提

- ローカル: `brew install ansible`(または pip install)
- VPS: SSH 鍵で root 権限相当の sudo が通る一般ユーザ(例: `ubuntu`)
- VPS にあらかじめ Caddy + (cross build を流す前なら他は不要)
- ドメインを VPS の IP に向けて DNS 反映済み

## 1. inventory を編集

```bash
$EDITOR deploy/ansible/inventory.yml
```

少なくとも次を実値に置換:

| 変数 | 例 |
|---|---|
| `ansible_host` | `napo.example.com` または IP |
| `ansible_user` | `ubuntu` |
| `napo_domain` | `napo.example.com` |
| `napo_caddy_email` | Let's Encrypt 通知用 |
| `napo_cf_team` | `tedsum.cloudflareaccess.com` |
| `napo_cf_aud` | CF Access App AUD |

`napo_cf_aud` は機微情報なので `ansible-vault` で暗号化推奨:

```bash
ansible-vault encrypt_string 'xxxxxxxx' --name 'napo_cf_aud'
# 出力ブロックを inventory.yml の vars セクションに貼る
```

## 2. 通信確認

```bash
ansible -i deploy/ansible/inventory.yml napo -m ping
# (vault 使うなら --ask-vault-pass を付ける)
```

## 3. プロビジョニング

```bash
ansible-playbook -i deploy/ansible/inventory.yml deploy/ansible/playbook.yml
# vault 使うなら:
#   ansible-playbook -i deploy/ansible/inventory.yml deploy/ansible/playbook.yml --ask-vault-pass
```

冪等なので再実行 OK。napo.env を変えたら再実行 → 自動で `systemctl restart napo`
が走る(バイナリ未配置のときは restart は失敗 ignore される)。

## 4. バイナリ + web bundle を deploy

```bash
HOST=ubuntu@napo.example.com PORT=22 mise run deploy
```

`deploy:web` で `static/dist/` を rsync → `build:release` で musl static-pie →
scp + `systemctl restart napo`。

## 5. 動作確認

```bash
ansible -i deploy/ansible/inventory.yml napo -a 'systemctl status napo'
mise run health URL=https://napo.example.com/healthz   # CF Access 前段で 401/403 になる場合あり
```

## 既存(手動)手順との関係

`deploy/README.md` の「初回セットアップ」bash 手順と等価。
- 手動でやりたい場合は引き続きそちらを参照
- Ansible 化後は再現性 + 別 VPS 展開で楽
