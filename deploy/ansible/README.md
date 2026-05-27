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
- ローカル: ufw / synchronize module を含む collection が必要なので、なければ:
  ```bash
  ansible-galaxy collection install community.general ansible.posix
  ```
  (brew の `ansible` メタパッケージなら既に同梱)
- VPS: SSH 鍵で root 権限相当の sudo が通る一般ユーザ(例: `ubuntu`)
- ドメインを VPS の IP に向けて DNS 反映済み(Cloudflare proxied = orange cloud ON)
- Caddy / ufw は ansible が apt で入れる(VPS は事前準備不要)

## 1. Cloudflare Origin Certificate を VPS に手動配置

CF Proxy + UFW(CF IP allowlist)構成だと Let's Encrypt の ACME チャレンジが
通らないので、CF が発行する Origin Certificate を Caddy に渡す。

cert/key を ansible で配ると「ローカルにファイルがある時だけ冪等性が成立」と
いう壊れた状態になるので、**VPS への配置は手動**。ansible は配置済みかどうかを
stat でチェックするだけ。

1. Cloudflare dashboard → 該当 zone → **SSL/TLS** → **Origin Server** →
   **Create Certificate**
2. デフォルト(RSA 2048、15 年、`*.<your-domain>` + `<your-domain>`)で生成
3. **Origin Certificate**(PEM)と **Private Key**(PEM)をコピー
   ※次の画面に二度と表示されないので必ず保存
4. CF dashboard → **SSL/TLS** → **Overview** → Encryption mode を
   **Full (strict)** に設定
5. VPS に SSH してファイル配置:

   ```bash
   # ローカル → VPS に scp
   scp <(pbpaste) <vps>:/tmp/origin.crt   # cert を Cmd+C しておいてから
   # または cert.pem ファイルがあればそれを scp

   # VPS で配置
   ssh <vps>
   sudo mkdir -p /etc/ssl/caddy
   sudo mv /tmp/origin.crt /etc/ssl/caddy/napo-origin.crt
   sudo mv /tmp/origin.key /etc/ssl/caddy/napo-origin.key   # key も同様
   sudo chown root:caddy /etc/ssl/caddy/napo-origin.*
   sudo chmod 0644 /etc/ssl/caddy/napo-origin.crt
   sudo chmod 0640 /etc/ssl/caddy/napo-origin.key
   ```

   配置先パスは inventory.yml の
   `napo_tls_cert_path` / `napo_tls_key_path` で変えられる(デフォルト
   `/etc/ssl/caddy/napo-origin.{crt,key}`)。

6. cert を更新したいときは VPS 上を直接書き換え(scp で上書き)→
   `sudo systemctl reload caddy`。

## 2. inventory を編集

```bash
$EDITOR deploy/ansible/inventory.yml
```

少なくとも次を実値に置換:

| 変数 | 例 |
|---|---|
| `ansible_host` | `napo.example.com` または IP |
| `ansible_user` | `ubuntu` |
| `napo_domain` | `napo.example.com` |
| `napo_cf_team` | `tedsum.cloudflareaccess.com` |
| `napo_cf_aud` | CF Access App AUD |

`napo_cf_aud` は機微情報なので `ansible-vault` で暗号化推奨:

```bash
ansible-vault encrypt_string 'xxxxxxxx' --name 'napo_cf_aud'
# 出力ブロックを inventory.yml の vars セクションに貼る
```

## 3. 通信確認

```bash
ansible -i deploy/ansible/inventory.yml napo -m ping
# (vault 使うなら --ask-vault-pass を付ける)
```

## 4. プロビジョニング

```bash
ansible-playbook -i deploy/ansible/inventory.yml deploy/ansible/playbook.yml
# vault 使うなら:
#   ansible-playbook -i deploy/ansible/inventory.yml deploy/ansible/playbook.yml --ask-vault-pass
```

冪等なので再実行 OK。napo.env を変えたら再実行 → 自動で `systemctl restart napo`
が走る(バイナリ未配置のときは restart は失敗 ignore される)。

## 5. バイナリ + web bundle を deploy

```bash
mise run deploy
```

内部で `web:build` → `build:release`(cross + Docker)→ `ansible-playbook deploy.yml`
の順に走る。ansible が sudo password を 1 回だけ求めて以降を保持する。

## 6. 動作確認

```bash
ansible -i deploy/ansible/inventory.yml napo -a 'systemctl status napo'
URL=https://napo.<your-domain>/healthz mise run health  # CF Access 前段で 302 になる
# ブラウザで https://napo.<your-domain> を開いて Google ログインまで通れば成功
```

## 既存(手動)手順との関係

`deploy/README.md` の「初回セットアップ」bash 手順と等価。
- 手動でやりたい場合は引き続きそちらを参照
- Ansible 化後は再現性 + 別 VPS 展開で楽
