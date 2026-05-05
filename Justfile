# napo-online タスクランナー(`brew install just` / `cargo install just`)
#
# デプロイ先は環境変数で上書き可能:
#   HOST="ubuntu@napo.example.com" PORT=22 just deploy
#
# デフォルトは README/docs を埋めるためのプレースホルダなので、必ず上書きすること。

set shell := ["bash", "-uc"]

host := env_var_or_default("HOST", "ubuntu@napo.example.com")
port := env_var_or_default("PORT", "22")
target := "x86_64-unknown-linux-musl"

# 全タスク一覧
default:
    @just --list

# Web (Vite+ + React) は mise tasks を参照: `mise tasks | grep web`

# ローカル開発: DEV_MODE で起動(ローカル DB は ./napo.db)
dev:
    NAPO_DEV_MODE=true NAPO_DB=./napo.db cargo run

# テスト
test:
    cargo test --all-targets

# fmt / clippy / test を一気に(CI と同じ)
ci:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test --all-targets

# クロスコンパイル
# macOS で musl-cross を入れるのは大変なので Docker + `cross` を使う方を推奨。
# 前提: `cargo install cross` + Docker (Docker Desktop / Rancher Desktop / OrbStack 等)
#       `rustup target add x86_64-unknown-linux-musl`
build-release:
    cross build --release --target {{target}}

# musl-cross をローカルに入れている人向けの直 build(参考)
build-release-native:
    cargo build --release --target {{target}}

# デプロイ: ビルド → scp → systemctl restart
deploy: build-release
    scp -P {{port}} target/{{target}}/release/napo-online {{host}}:/tmp/
    ssh -p {{port}} {{host}} ' \
        sudo install -m 755 -o napo -g napo \
            /tmp/napo-online /opt/napo/napo-online.new && \
        sudo mv -f /opt/napo/napo-online /opt/napo/napo-online.prev 2>/dev/null || true && \
        sudo mv /opt/napo/napo-online.new /opt/napo/napo-online && \
        sudo systemctl restart napo \
    '

# ロールバック(.prev に戻す)
rollback:
    ssh -p {{port}} {{host}} ' \
        sudo mv /opt/napo/napo-online /opt/napo/napo-online.broken && \
        sudo mv /opt/napo/napo-online.prev /opt/napo/napo-online && \
        sudo systemctl restart napo \
    '

# ログ追跡
logs:
    ssh -p {{port}} {{host}} 'sudo journalctl -u napo -f'

# /healthz への疎通確認(本番ドメインへ)
ping URL='https://napo.example.com/healthz':
    curl -sS -o /dev/null -w 'HTTP %{{http_code}}\n' {{URL}}

# DB をローカルに引いて確認
db-pull:
    scp -P {{port}} {{host}}:/var/lib/napo/game.db ./game.prod.db
    @echo "downloaded ./game.prod.db"
