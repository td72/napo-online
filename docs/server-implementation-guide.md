# ナポレオンサーバ 実装ガイド

身内でナポレオン(トランプ)を遊ぶためのオンライン対戦サーバの実装ガイドです。
ゲームロジックは別リポジトリの `napo-core` クレートに実装済み(または並行開発中)で、
本ドキュメントはそれを Git 依存として取り込んでホストするネットワークサーバの設計と実装方針を扱います。

> **前提**: `napo-core` と `napo-online` は同じ人が書く。
> ドキュメントは「両方の整合を取りながら進める」立場で記述する。

---

## 1. プロジェクトの全体像

### 1.1 目的・スコープ

- **対象ユーザ**: 開発者と数名の友達(同時 5〜10人)
- **遊び方**: 普段プレイしているハウスルール込みのナポレオン
- **公開範囲**: 招待制(部外者は技術的に到達不可)
- **求めるもの**: 月額固定で安く、課金事故なしで放置運用できる

### 1.2 物理構成

```
┌──────────────┐
│  Player      │  ブラウザ(SPA)
│  (Browser)   │
└───────┬──────┘
        │ HTTPS / WSS
        ▼
┌─────────────────────────────────────────┐
│  Cloudflare Edge                        │
│  - DNS proxy (Proxied)                  │
│  - TLS termination (auto)               │
│  - Cloudflare Access (Google ログイン)  │
│  - WAF / Bot Fight / 国別制限           │
└───────┬─────────────────────────────────┘
        │ HTTPS (Cloudflare IP のみ通過)
        ▼
┌─────────────────────────────────────────┐
│  さくら VPS 1G(石狩, Ubuntu 24.04)      │
│  ┌─────────────────────────────────┐    │
│  │ Caddy(443 で待受、HTTPS終端)    │    │
│  │   ↓ reverse_proxy 127.0.0.1:8080 │    │
│  │ napo-online (axum + tokio)   │    │
│  │   ↓ Git依存                       │    │
│  │ napo-core (別リポジトリ)     │    │
│  │   ↓                              │    │
│  │ SQLite (rusqlite, 単一ファイル)  │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

### 1.3 リクエストの一生

```
1. ユーザが https://napo.<your-domain> を開く
2. Cloudflare が Access ゲートを起動 → Google ログインへ誘導
3. 許可リスト(Emails policy)を通過したら JWT を Cookie 化して VPS へ
4. Caddy が HTTPS を終端、WebSocket も含めて 127.0.0.1:8080 へ転送
5. axum が WebSocket をハンドルし、メッセージを部屋(Room)アクターへ
6. 部屋アクターが napo-core で状態遷移を計算
7. 各プレイヤー宛の差分(視点別)をブロードキャスト
```

### 1.4 採用済みの設計判断(変更不可前提)

| 領域 | 決定事項 | 理由 |
|---|---|---|
| インフラ | さくらVPS 1G 石狩 月額固定 | 課金事故ゼロ、レイテンシ国内 |
| OS | Ubuntu 24.04 LTS | 1G プランで対応可、長期サポート |
| TLS | Caddy + Cloudflare Universal SSL | ゼロ設定、自動更新 |
| 認証 | Cloudflare Access + Google OAuth | アプリ非依存、許可リスト方式 |
| 言語 | Rust | 安全性、性能、シングルバイナリ |
| Web | axum 0.7+ | tokio 公式系、エコシステム充実 |
| DB | SQLite (rusqlite または sqlx) | 1ファイル、運用ゼロ |
| プロセス管理 | systemd | Ubuntu 標準、リソース上限が容易 |
| ドメイン | Cloudflare Registrar | 卸価格、WHOIS 自動匿名化 |
| **コード分離** | **napo-core / napo-online を別リポジトリ** | core を他用途(CLI、テストハーネス、別UI)へ流用可能に |

---

## 2. リポジトリ構成

### 2.1 2リポジトリ構成

```
github.com/<you>/napo-core           ← private repo
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── card.rs
    ├── deck.rs
    ├── bid.rs
    ├── trick.rs
    ├── game.rs
    └── view.rs

github.com/<you>/napo-online         ← private repo(本ドキュメントの対象)
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── auth.rs
│   ├── http.rs
│   ├── ws.rs
│   ├── room.rs
│   ├── hub.rs
│   ├── protocol.rs
│   ├── db.rs
│   ├── error.rs
│   └── game_adapter.rs    # core との橋渡し
├── static/                # フロント成果物の配置先(別途生成)
└── deploy/
    ├── Caddyfile.example
    ├── napo.service
    └── napo.env.example
```

### 2.2 napo-core 側の Cargo.toml(現状)

```toml
[package]
name = "napo-core"
version = "0.1.0"
edition = "2024"

[features]
default = []
serde = ["dep:serde"]

[dependencies]
rand = "0.10.1"
serde = { version = "1", features = ["derive"], optional = true }
thiserror = "2"

[dev-dependencies]
serde_json = "1"
static_assertions = "1"
```

> **注**: 乱数は `napo-core` 内部で `seed: u64` を保持して `StdRng` を生成する方式。
> 「サーバ側から RNG を注入」する旧設計案ではない。決定論性は同じ `(Rules, seed, Action 列)` で再現される。

### 2.3 napo-online の Cargo.toml

`napo-core` の依存(`rand 0.10` / `thiserror 2` / edition 2024)に揃える。
**新規追加時は必ず `cargo add <crate> [--features ...]` で最新 stable を取る**。
以下は 2026-05 時点で `cargo add` を回した結果なので、新しい開発では crate ごとに `cargo search` か `cargo add` で更新してから写すこと。

```toml
[package]
name = "napo-online"
version = "0.1.0"
edition = "2024"

[dependencies]
# ゲームロジック(serde feature で JSON 永続化を可能に)
napo-core = { path = "../napo-core", features = ["serde"] }
# napo-core が tag を切ったら以下に置き換える(開発時は path、リリース時は git tag、A10):
# napo-core = { git = "ssh://git@github.com/td72/napo-core.git", tag = "vX.Y.Z", features = ["serde"] }
# [patch."ssh://git@github.com/td72/napo-core.git"]
# napo-core = { path = "../napo-core" }

# Web / async
axum = { version = "0.8", features = ["ws", "macros"] }
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "compression-zstd", "fs"] }

# シリアライゼーション
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 認証(Cloudflare Access JWT 検証)
jsonwebtoken = "10"
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls"] }
# 注: reqwest 0.13 で feature 名が `rustls-tls` → `rustls` にリネームされた

# ロギング・エラー
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
anyhow = "1"
thiserror = "2"

# DB
rusqlite = { version = "0.39", features = ["bundled"] }

# 設定・ユーティリティ
dotenvy = "0.15"
uuid = { version = "1", features = ["v4", "serde"] }
rand = "0.10"
dashmap = "6"
# 注: dashmap 7 系は 2026-05 時点で RC 段階。安定版が出るまで 6 を維持

[dev-dependencies]
# 統合テスト/サンプル WS クライアント用
tokio-tungstenite = "0.29"
futures-util = "0.3"
```

> **依存方針(A10 で確定)**: 開発中は `napo-core` を `path = "../napo-core"` で参照、
> napo-core が tag を切ったら git **tag 固定** + `[patch]` でローカル差し替えに切り替える。
> `Cargo.lock` をコミットして版を固定する。
>
> **更新運用**: メジャー版が上がったら `cargo update -p <crate>` ではなく
> `cargo add <crate>@<新版>` で明示的にバンプし、API 互換のテストを通す。

### 2.4 各モジュールの責務(napo-online)

| モジュール | 責務 |
|---|---|
| `main.rs` | エントリポイント、tracing 初期化、ルータ構築、起動 |
| `config.rs` | 環境変数の読み込みと型化(`bind`, `db_path`, `cf_team`, `aud`, ...) |
| `auth.rs` | Cloudflare Access の JWT 検証、JWKS キャッシュ |
| `http.rs` | 通常 HTTP ルート(健全性確認、静的配信、RESTish API) |
| `ws.rs` | `/ws` の WebSocket アップグレードハンドラ、接続単位ループ |
| `room.rs` | 1部屋 = 1 tokio タスクのアクター、ゲーム状態の唯一の所有者 |
| `hub.rs` | 全部屋を管理する Map(部屋ID → Room ハンドル)、ロビー機能 |
| `protocol.rs` | クライアント・サーバ間メッセージの型定義(`serde` derive) |
| `game_adapter.rs` | `napo-core` の型を `protocol` の wire 型に変換するアダプタ |
| `db.rs` | SQLite 接続、マイグレーション、永続化処理 |
| `error.rs` | 統一エラー型、`IntoResponse` 実装 |

---

## 3. Git 依存(private リポ)を使うための準備

`napo-core` が GitHub private リポジトリの場合、ローカル開発・CI・VPS デプロイの 3 箇所で
アクセス権が必要になる。

### 3.1 アクセス手段の選択肢

| 方法 | 用途 | 備考 |
|---|---|---|
| **SSH 鍵(個人)** | ローカル開発 | `~/.ssh/id_ed25519` を GitHub に登録、`Cargo.toml` は `ssh://git@github.com/...` |
| **Deploy key(リポジトリ単位)** | CI / VPS | リポジトリ毎に鍵を発行、`napo-core` に read-only deploy key を登録 |
| **Personal Access Token(PAT)** | HTTPS 経由 | 取り回しはラクだが期限管理と漏洩リスクあり |
| **Fine-grained PAT** | 推奨 | リポジトリと権限を細かく絞れる |

推奨: **ローカル = 個人SSH鍵、CI/VPS = Deploy key(read-only)**。

### 3.2 SSH 経由の Cargo 設定

`Cargo.toml`:
```toml
[dependencies]
napo-core = { git = "ssh://git@github.com/<you>/napo-core.git", branch = "main" }
```

または(タグで固定する場合):
```toml
napo-core = { git = "ssh://git@github.com/<you>/napo-core.git", tag = "v0.2.0" }
```

`~/.cargo/config.toml`:
```toml
[net]
git-fetch-with-cli = true   # ssh-agent と認識される鍵を素直に使ってくれる
```

`git-fetch-with-cli = true` を入れないと `cargo` が内蔵の libgit2 でフェッチしてしまい、
SSH エージェント認識が不安定になることがある。

### 3.3 ローカル開発の効率化:`[patch]` でローカルパスに差し替え

両リポを同時に編集する局面では、毎回 `git push` してから `cargo update` するのは面倒。
ローカルチェックアウトに差し替える `[patch]` セクションを使うと便利:

```toml
# napo-online/Cargo.toml の末尾
[patch."ssh://git@github.com/<you>/napo-core.git"]
napo-core = { path = "../napo-core" }
```

ローカルで両リポを `~/work/napo-core` と `~/work/napo-online` のように並べておけば、
`napo-core` を編集して保存 → `cargo build` で即座に反映される。

`[patch]` を入れたまま push しないよう、開発者ローカル専用にしたい場合は
`Cargo.toml` から外して、代わりに `.cargo/config.toml`(ワークスペース ローカル)に書く方法もある:

```toml
# napo-online/.cargo/config.toml(.gitignore に追加)
[patch."ssh://git@github.com/<you>/napo-core.git"]
napo-core = { path = "../napo-core" }
```

### 3.4 VPS でビルドする場合の Deploy key 設定

VPS 上で `cargo build` する場合(クロスコンパイルではなく VPS 直ビルド):

```bash
# VPS で deploy 用 SSH 鍵を生成
sudo -u napo ssh-keygen -t ed25519 -f /home/napo/.ssh/id_napo_core -N ""
sudo -u napo cat /home/napo/.ssh/id_napo_core.pub
# ↑ 出力を GitHub の napo-core リポジトリの Settings → Deploy keys に登録(read-only)

# SSH config
sudo -u napo tee /home/napo/.ssh/config <<'EOF'
Host github-napo-core
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_napo_core
    IdentitiesOnly yes
EOF
```

`Cargo.toml` 側で `Host github-napo-core` を使う書き方:
```toml
napo-core = { git = "ssh://git@github-napo-core/<you>/napo-core.git", branch = "main" }
```

ただし **VPS でビルドする運用は重い**(メモリ・依存解決時間)ので、**ローカルでクロスコンパイルして
バイナリだけ scp する方式を推奨**(§13)。その場合は VPS に Deploy key を置く必要すらない。

### 3.5 推奨フロー

| 環境 | 取得方法 |
|---|---|
| ローカル | 個人 SSH 鍵 + 開発中は `[patch]` でローカル差し替え |
| CI(GitHub Actions) | リポジトリの secrets にしまった Deploy key |
| **VPS** | **直ビルドしない**(ローカルでクロスコンパイル → scp) |

---

## 4. ゲームロジックライブラリ(napo-core)の取り扱い

### 4.1 サーバが期待する性質(現状の napo-core で満たされているもの)

`napo-core` は以下を提供しており、サーバ側は **「これらを正しく呼ぶ」** ことに集中する。

1. **決定論的**: 同じ `(Rules, seed, Action 列)` → 同じ局面・スコアが完全再現。
   乱数は `napo-core` 内部で `seed: u64` を保持して `StdRng` を生成する方式
   (旧構想にあった「サーバから RNG 注入」ではない)
2. **直列化可能**: `serde` feature を有効化すると `Game` / `Match` 全体を JSON ラウンドトリップ可能。
   `SCHEMA_VERSION: u32` が同梱され、互換性チェックに使える
3. **視点別ビュー**: `Game::observation(player) -> Observation` で情報秘匿済みのビューが取れる。
   - 自分の手札・公開済み履歴・場の進行状況のみ
   - `WidowExchange` フェーズの declarer のみ `visible_widow` が見える
   - 場札交換で捨てた絵札は `revealed_face_discards` で全員に公開
4. **アクション駆動 API**: `legal_actions()` で適法手のみ列挙される。サーバ/UI は
   この中から選んで `apply` に渡せば、不正手は型レベルに排除される
5. **完全な履歴保持**: `Game::action_log() -> &[(usize, Action)]` で全 Action 履歴。
   `Game::replay(rules, seed, actions)` / `Game::step_iter(...)` で同一の局を再構築
6. **エラー型がドメインを表現**: `Error::IllegalBid` / `Error::MustFollowSuit` /
   `Error::PlayerDoesNotHold` などをサーバ側で握り直して `ServerMessage::Error` に翻訳
7. **`Send + Sync`**: 主要型がスレッド安全(コンパイル時 assert 済)。tokio タスク間で安全に渡せる

### 4.1.1 napo-core が **持っていない** もの(server 側で吸収する)

| 不足 | サーバ側の対応 |
|---|---|
| **`Event` enum** がない | `apply` 前後の `Phase` / `Observation` を比較して `PublicEvent` を server で組み立てる(§4.3) |
| **観戦者ビュー** がない | v1 では非対応(A6)。v2 で `Observation::spectator()` 相当を core に追加する想定 |
| **席の決め方** | core は `seat: usize` を扱うだけ。席ランダム化(A2)・PlayerId ↔ seat マッピングは server 側 |
| **タイムアウト** | core は時間概念なし。1手の制限時間や離席判定は server 側(§6.4) |
| **AI / forfeit** | core にプレイヤー代行は無い。離席時はホストが「abort hand」を選ぶ運用(A5) |

### 4.2 ライブラリの公開 API(現状)

`napo-core` の公開型(`napo_core::*`)。識別子は `usize`(配列インデックス用)、
`Match` で複数局のセッション、`Game` で1局を表す。

```rust
// napo-core/src/lib.rs(実装)

pub mod action;
pub mod bid;
pub mod card;
pub mod deck;
pub mod error;
pub mod game;
pub mod match_set;
pub mod rules;
pub mod score;
pub mod state;
pub mod trick;

pub const SCHEMA_VERSION: u32 = 1;

pub use action::Action;
pub use bid::{Bid, Trump};
pub use card::{Card, Rank, Suit};
pub use error::{Error, Result};
pub use game::{Game, Observation};
pub use match_set::Match;
pub use rules::Rules;
pub use state::Phase;
```

#### Action(プレイヤー行動)

```rust
pub enum Action {
    Bid(Bid),                              // Bidding フェーズ
    Pass,                                  // Bidding フェーズ
    DeclareAdjutant(Card),                 // DeclareAdjutant フェーズ(declarer のみ)
    ExchangeWidow { discard: Vec<Card> },  // WidowExchange フェーズ(declarer のみ、widow_size 枚)
    PlayCard(Card),                        // Playing フェーズ(current_player のみ)
}

pub struct Bid {
    pub tricks: u8,                        // 13〜20
    pub trump: Trump,
}

pub enum Trump {
    Suit(Suit),
    NoTrump,
}
```

#### Phase(状態機械)

進行は `Bidding → DeclareAdjutant → WidowExchange → Playing → Finished`。
全員パスで `Bidding` に戻る(配り直し)。

```rust
pub enum Phase {
    Bidding {
        current_bid: Option<(usize, Bid)>,
        passed: Vec<bool>,
        next: usize,
    },
    DeclareAdjutant { declarer: usize, bid: Bid },
    WidowExchange { declarer: usize, bid: Bid, adjutant_card: Card },
    Playing {
        declarer: usize,
        bid: Bid,
        adjutant_card: Card,
        adjutant_player: Option<usize>,    // 副官カードがプレイされて発覚するまで None
        next: usize,
        current_trick: Vec<PlayedCard>,
        completed: Vec<CompletedTrick>,
    },
    Finished {
        scores: Vec<i32>,
        declarer: usize,
        bid: Bid,
        adjutant_card: Card,
        adjutant_player: Option<usize>,
        tricks: Vec<CompletedTrick>,
    },
}
```

#### Game(1局)

```rust
impl Game {
    pub fn new(rules: Rules, seed: u64) -> Self;
    pub fn with_first_bidder(rules: Rules, seed: u64, first_bidder: usize) -> Self;
    pub fn replay(rules: Rules, seed: u64, actions: &[Action]) -> Result<Self>;
    pub fn step_iter(rules: Rules, seed: u64, actions: &[Action])
        -> impl Iterator<Item = Result<(Game, Action)>>;

    pub fn apply(&mut self, action: Action) -> Result<()>;
    pub fn legal_actions(&self) -> Vec<Action>;

    pub fn observation(&self, player: usize) -> Observation;
    pub fn action_log(&self) -> &[(usize, Action)];

    pub fn phase(&self) -> &Phase;
    pub fn current_player(&self) -> Option<usize>;
    pub fn is_finished(&self) -> bool;
    pub fn scores(&self) -> Option<&[i32]>;
    pub fn rules(&self) -> &Rules;
    pub fn seed(&self) -> u64;
    pub fn schema_version(&self) -> u32;
}
```

#### Observation(視点別ビュー)

```rust
pub struct Observation {
    pub player: usize,
    pub num_players: usize,
    pub hand: Vec<Card>,                          // 自分の手札のみ
    pub bid_history: Vec<(usize, Option<Bid>)>,   // None = Pass
    pub current_bid: Option<(usize, Bid)>,
    pub declarer: Option<usize>,
    pub adjutant_card: Option<Card>,              // declarer or 発覚後のみ
    pub adjutant_player: Option<usize>,           // 副官カードがプレイされて発覚した時点で Some
    pub completed: Vec<CompletedTrick>,
    pub current_trick: Vec<PlayedCard>,
    pub trick_index: usize,
    pub to_act: Option<usize>,
    pub visible_widow: Option<Vec<Card>>,         // WidowExchange の declarer のみ
    pub revealed_face_discards: Vec<Card>,        // Playing 以降、全員に公開
}
```

#### Match(複数局のセッション)

```rust
impl Match {
    pub fn new(rules: Rules, base_seed: u64) -> Self;

    pub fn current_hand(&self) -> &Game;
    pub fn current_hand_mut(&mut self) -> &mut Game;
    pub fn apply(&mut self, action: Action) -> Result<()>;
    pub fn start_next_hand(&mut self) -> Result<()>;  // current_hand が finished でなければ Err

    pub fn cumulative_scores(&self) -> &[i32];
    pub fn hand_count(&self) -> usize;
    pub fn completed_hands(&self) -> &[Game];
}
```

各局の seed = `base_seed + hand_index`、最初のビッダー seat も局ごとに +1 で回転(core が自動でやる)。

#### Rules(ハウスルールフラグ)

```rust
pub struct Rules {
    pub num_players: usize,                       // 標準: 5
    pub hand_size: usize,                         // 標準: 10
    pub widow_size: usize,                        // 標準: 2
    pub min_declared_tricks: u8,                  // 標準: 13
    pub max_declared_tricks: u8,                  // 標準: 20
    pub suit_tiebreak_order: [Suit; 4],
    pub no_trump_strongest: bool,                 // 標準: true
    pub allow_no_trump: bool,                     // 標準: true
    pub allow_self_adjutant: bool,                // 標準: true
    pub yoromeki: bool,                           // 標準: true
    pub same2: bool,                              // 標準: true
    pub seijack_urajack: bool,                    // 標準: true
}
```

サーバは `Rules::default()` 固定で起動する(A1: 5人固定)。`num_players * hand_size + widow_size == 52` という制約があり、`Rules::validate()` でチェック。

#### Error

```rust
pub enum Error {
    InvalidRules(String),
    GameFinished,
    UnexpectedAction { expected: &'static str, actual: Action },
    IllegalBid(Bid),
    BidNotHighEnough,
    WrongDiscardSize { expected: usize, actual: usize },
    DeclarerDoesNotHold(Card),
    DuplicateDiscard,
    PlayerDoesNotHold { player: usize, card: Card },
    MustFollowSuit,
    CurrentHandNotFinished,
}
```

> **API の整合**: 上記は `napo-core` v0.1.0 の実装と一致する。core を bump したらこのセクションを更新する。

### 4.3 game_adapter.rs の役割

`napo-core` には `Event` enum が無いので、サーバ側で **「`apply` 前後の `Phase` / `Observation` を比較してイベントを組み立てる」** 必要がある。
このアダプタが core と wire(`protocol.rs`)の橋渡しをする。

```rust
// napo-online/src/game_adapter.rs

use napo_core::{self as core, Action, Match, Observation, Phase};
use std::collections::HashMap;
use crate::protocol::{PublicEvent, GameView, WireAction};

pub struct GameSession {
    pub id: SessionId,
    pub session: core::Match,                      // 10局1セット(A4)
    pub seat_to_player: HashMap<usize, PlayerId>,  // 入室順をシャッフルして決定(A2)
    pub player_to_seat: HashMap<PlayerId, usize>,
    /// 直前の Phase。apply 前にスナップショットして diff に使う
    last_phase: Phase,
}

pub struct ApplyOutcome {
    /// 全員にブロードキャストする公開イベント(アニメーション用)
    pub events: Vec<PublicEvent>,
    /// 各プレイヤーへの最新 Observation(状態同期用)
    pub observations: HashMap<PlayerId, Observation>,
    /// この apply で局が終わったか
    pub hand_finished: bool,
    /// この apply でセット(10局)が終わったか
    pub match_finished: bool,
}

impl GameSession {
    pub fn apply(&mut self, player: PlayerId, action: WireAction)
        -> Result<ApplyOutcome, ServerError>
    {
        let seat = *self.player_to_seat.get(&player).ok_or(ServerError::NotInGame)?;
        let core_action = action.into_core(self.session.current_hand())?;

        // current_player との一致を確認
        if self.session.current_hand().current_player() != Some(seat) {
            return Err(ServerError::NotYourTurn);
        }

        let before_phase = self.session.current_hand().phase().clone();
        self.session.apply(core_action.clone())?;
        let after_phase = self.session.current_hand().phase().clone();

        let events = self.derive_events(seat, &core_action, &before_phase, &after_phase);
        let observations = self.observations_for_all();
        let hand_finished = self.session.current_hand().is_finished();
        let match_finished = hand_finished
            && self.session.hand_count() + 1 >= MATCH_HAND_COUNT;

        self.last_phase = after_phase;
        Ok(ApplyOutcome { events, observations, hand_finished, match_finished })
    }

    /// 全員が "Next" を押した後に呼ぶ(§6.x の Next 投票で集まった時)
    pub fn start_next_hand(&mut self) -> Result<ApplyOutcome, ServerError> {
        self.session.start_next_hand()?;
        Ok(ApplyOutcome {
            events: vec![PublicEvent::HandStarted {
                hand_index: self.session.hand_count(),
                first_bidder: self.first_bidder_of_current_hand(),
            }],
            observations: self.observations_for_all(),
            hand_finished: false,
            match_finished: false,
        })
    }

    /// 配り直し(全員パス)時の特別処理。Phase が Bidding → Bidding に戻ったことを検出
    fn detect_redeal(&self, before: &Phase, after: &Phase) -> bool {
        matches!((before, after),
            (Phase::Bidding { .. }, Phase::Bidding { passed, .. }) if passed.iter().all(|p| !*p))
    }

    fn derive_events(&self, seat: usize, action: &Action, before: &Phase, after: &Phase) -> Vec<PublicEvent> {
        let mut out = Vec::new();
        match (action, after) {
            (Action::Bid(b), _)        => out.push(PublicEvent::Bidded { seat, bid: b.into() }),
            (Action::Pass, _)          => out.push(PublicEvent::Passed { seat }),
            (Action::DeclareAdjutant(c), _) => {
                // 副官カード自体は WidowExchange までは公開、副官プレイヤーは伏せる
                out.push(PublicEvent::AdjutantDeclared { card: (*c).into() });
            }
            (Action::ExchangeWidow { discard }, _) => {
                let face_only: Vec<_> = discard.iter().filter(|c| c.is_face()).copied().collect();
                out.push(PublicEvent::WidowExchanged { revealed_face_cards: face_only.into() });
            }
            (Action::PlayCard(c), Phase::Playing { adjutant_player, .. }) => {
                out.push(PublicEvent::CardPlayed { seat, card: (*c).into() });
                // この play で副官が発覚したか
                if adjutant_player == &Some(seat) && /* 副官カードと一致 */ true {
                    out.push(PublicEvent::AdjutantRevealed { seat });
                }
            }
            _ => {}
        }
        // フェーズ遷移由来のイベント
        match (before, after) {
            (Phase::Bidding { .. }, Phase::DeclareAdjutant { declarer, bid }) => {
                out.push(PublicEvent::NapoleonDecided { seat: *declarer, bid: bid.into() });
            }
            (Phase::Playing { current_trick: t, .. }, Phase::Playing { completed: c, next, .. })
                if c.last().map(|x| x.plays.len()) == Some(self.session.rules().num_players) =>
            {
                out.push(PublicEvent::TrickWon { seat: *next });
            }
            (Phase::Playing { .. }, Phase::Finished { scores, .. }) => {
                out.push(PublicEvent::HandEnded { scores: scores.clone() });
            }
            _ => {}
        }
        // 配り直し検出
        if self.detect_redeal(before, after) {
            out.push(PublicEvent::HandRedealt);
        }
        out
    }

    fn observations_for_all(&self) -> HashMap<PlayerId, Observation> {
        self.player_to_seat.iter()
            .map(|(p, &s)| (p.clone(), self.session.current_hand().observation(s)))
            .collect()
    }
}
```

このアダプタが握る「**core が知らないこと**」:
- どのプレイヤー(email/PlayerId)がどの seat か(ランダム配席、A2)
- 接続先 `mpsc::Sender<ServerMessage>` への配線
- wire プロトコルのバージョニング(`v: 1`)
- セット長(10局)・Next 投票の状態
- 配り直し検出・副官発覚検出など、core の Phase 遷移から派生するイベント

> **設計原則(A8 で確定)**: `apply` 1回ごとに **`Vec<PublicEvent>`(差分)+ 全員分の `Observation`(全体)を両方送る**。
> クライアントは Event でアニメ → 直後の Observation で「これが正」と上書き同期する。
> Event 取りこぼしや順序逆転が起きても、次の Observation で自動修復される。

---

## 5. ネットワークプロトコル設計

### 5.1 トランスポート

- 全通信は **WebSocket(WSS)** に集約。
- 通常 HTTP は健全性確認(`GET /healthz`)と静的ファイル配信のみ。
- メッセージ形式は **JSON**(デバッグ容易性優先)。性能が必要になったら MessagePack へ。

### 5.2 メッセージのフレーム

```jsonc
{
  "v": 1,                           // プロトコルバージョン
  "id": "01HW...",                  // メッセージID(任意、相関に使う)
  "type": "play_card",              // 識別子
  "payload": { "card": "SA" }       // Card は1文字スート + ランク英字(A9)
}
```

#### Card / Bid の wire 表現(A9 で確定)

- **Card**: `"SA"` 形式(1文字スート + ランク英字)。`napo-core` の `Card::FromStr` が
  `"SA"` `"♠A"` `"spade-ace"` の3形式を受け付けるが、サーバ送信は **`"SA"` 固定**。
  - スート: `S`(♠) / `H`(♥) / `D`(♦) / `C`(♣)
  - ランク: `A K Q J 10 9 8 7 6 5 4 3 2` (`napo-core::card::Rank` の Display に準拠)
- **Bid**: `{ "tricks": 13, "trump": "S" }` または `{ "tricks": 13, "trump": "NT" }`(NoTrump)
- **Suit**: `"S" "H" "D" "C"`

#### メッセージ enum

```rust
// protocol.rs
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ClientMessage {
    // ロビー
    Hello { display_name: String },
    CreateRoom,
    JoinRoom { room_id: String },
    LeaveRoom,

    // ホスト操作(部屋作成者のみ受理)
    StartMatch,                          // 5人揃ったら 10局1セットを開始
    AbortHand,                           // 離席が長引いた時の局中断(T3 経過後のみ)
    EndMatch,                            // セット終了(T4 経過後のみ)

    // 全員操作
    NextHand,                            // 局終了後の "Next" 投票(全員揃うと次の局へ、A3)

    // ゲーム行動(現フェーズに応じた variant のみ受理)
    SubmitAction { action: WireAction },

    Ping,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WireAction {
    Bid { tricks: u8, trump: WireTrump },
    Pass,
    DeclareAdjutant { card: String },             // "SA"
    ExchangeWidow { discard: Vec<String> },       // ["SA", "H10"]
    PlayCard { card: String },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum WireTrump { S, H, D, C, NT }

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    // 接続/ロビー
    Welcome { player_id: PlayerId, server_version: String },
    RoomState { state: RoomView },               // 入退室・ホスト・ready 状態
    Error { code: String, message: String },
    Pong,

    // ゲーム進行(A8: Event と Observation を両方送る)
    Event { event: PublicEvent },                // アニメーション用の差分
    Observation { observation: WireObservation },// 各プレイヤーへ最新の正しい状態(個別配信)

    // セット管理
    MatchStarted { hand_count: u32, seats: Vec<SeatAssignment> },  // 席ランダム配置の結果
    HandEnded { scores: Vec<i32>, cumulative: Vec<i32> },
    NextHandReady { ready_seats: Vec<usize> },   // Next 投票の進捗
    MatchEnded { final_scores: Vec<i32> },

    // 切断/再接続
    PlayerDisconnected { seat: usize, since_ms: u64 },
    PlayerReconnected { seat: usize },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum PublicEvent {
    HandStarted { hand_index: u32, first_bidder: usize },
    Bidded { seat: usize, bid: WireBid },
    Passed { seat: usize },
    NapoleonDecided { seat: usize, bid: WireBid },
    AdjutantDeclared { card: String },           // 副官カードは公開、副官プレイヤーは伏せる
    WidowExchanged { revealed_face_cards: Vec<String> },  // 絵札のみ公開
    CardPlayed { seat: usize, card: String },
    AdjutantRevealed { seat: usize },            // 副官カードがプレイされた瞬間
    TrickWon { seat: usize },
    HandEnded { scores: Vec<i32> },
    HandRedealt,                                 // 全員パスで配り直し(A7)
}
```

`Suit` / `Card` / `Bid` は `napo-core` の型を直接 re-export せず、wire 用の文字列/数値表現に
`game_adapter` で変換する(プロトコルの安定化のため)。

### 5.3 接続シーケンス

```
Client                          Server
  │  WS Upgrade /ws              │
  ├─────────────────────────────▶│  CF Access JWT を Cookie/Header から取得
  │                              │  email を抽出 → PlayerId に解決(初回はDB登録)
  │  Welcome                     │
  │◀─────────────────────────────┤
  │  CreateRoom or JoinRoom      │
  ├─────────────────────────────▶│  Hub.create_room() / Hub.join(room_id, player)
  │  RoomState                   │
  │◀─────────────────────────────┤  入室通知を他参加者にもブロードキャスト
  │                              │
  │  (5人揃ったら) ホストが       │
  │  StartMatch                  │
  ├─────────────────────────────▶│  Room actor が席をランダム配置(A2)、
  │                              │  core::Match::new(Rules::default(), seed) で 1局目開始
  │  MatchStarted { seats }      │
  │  + Observation (個別)        │
  │◀─────────────────────────────┤  各プレイヤーに席割と自視点を送る
  │                              │
  │  SubmitAction { Bid 13 ... } │
  ├─────────────────────────────▶│  Match::apply(Bid) → game_adapter で events 構築
  │  Event { Bidded(...) }       │  ← 全員に同じ
  │  Observation (個別)          │  ← 各人に最新ビュー(A8 二重送信)
  │◀─────────────────────────────┤
  │                              │
  │  ... (Bid/Pass、副官、widow、play を繰り返し)
  │                              │
  │  Event { HandEnded { ... } } │  局終了
  │  HandEnded { scores, cumul } │
  │◀─────────────────────────────┤
  │  NextHand                    │  各人が "Next" を押す(A3 (ii))
  ├─────────────────────────────▶│  Room が ready_seats を集計、全員揃えば次局開始
  │  NextHandReady               │  途中経過を全員に流す
  │◀─────────────────────────────┤
  │  Event { HandStarted ... }   │  全員揃ったら自動で次局
  │  Observation (個別)          │
  │◀─────────────────────────────┤
  │                              │
  │  ... (10局完了まで繰り返し)
  │                              │
  │  MatchEnded { final_scores } │  10局終了 or ホスト EndMatch
  │◀─────────────────────────────┤
```

### 5.4 視点別ブロードキャストの原則

- **手札情報をクライアントに送るのは本人だけ**。`PrivateEvent` で個別配信。
- ナポレオン宣言時の副官カード自体は公開、だが「**誰が副官か**」は副官カードが場に出るまで秘匿。
- サーバ側は `core::Game::view(seat)` を使って各プレイヤー専用の状態を生成し、それぞれの送信チャンネルに流す。
- クライアントを信頼せず、**サーバが authoritative**。クライアントから来るのは「アクション」のみ。

### 5.5 Ping/Pong と切断検知

- クライアントが 30秒ごとに `Ping` を送信。サーバは `Pong` で応答。
- 60秒応答なし → サーバ側でセッションを切断。
- 切断時はゲームを停止せず「離席」状態にして、再接続を待つ(§6.4)。

### 5.6 メッセージサイズ・レート制限

- 1メッセージ最大 **8 KB**。これを超えたら即切断。
- 1接続あたり **10 msg/sec**。バーストは20までバケットで吸収。
- `tower_governor` または自前のトークンバケットを `ws.rs` 内で実装。

---

## 6. 部屋(Room)とハブ(Hub)の設計

### 6.1 アクターモデル

「**1部屋 = 1 tokio タスク**」が原則。状態は1タスクが排他所有するので、ロックは不要。
他のタスクとは MPSC チャンネル経由でメッセージをやり取りする。

```rust
// room.rs
pub struct Room {
    id: RoomId,
    host: PlayerId,                         // 部屋作成者(StartMatch / AbortHand / EndMatch を発火可能)
    state: RoomPhase,
    seats: Vec<Option<SeatedPlayer>>,       // index = seat、None = 空席。長さ = 5(A1)
    listeners: HashMap<PlayerId, mpsc::Sender<ServerMessage>>,
    session: Option<GameSession>,           // game_adapter::GameSession(10局1セット, A4)
    next_ready: HashSet<usize>,             // Next 投票で押した seat(A3 (ii))
    rx: mpsc::Receiver<RoomCommand>,
}

pub struct SeatedPlayer {
    pub player_id: PlayerId,
    pub display_name: String,
    pub state: PlayerConnState,
}

pub enum PlayerConnState {
    Online,
    Suspended { since: Instant },           // 切断中(再接続待ち、§6.4)
}

pub enum RoomPhase {
    Lobby,                                  // 5人未満 or StartMatch 待ち
    Playing,                                // セット進行中
    BetweenHands { hand_index: u32 },       // 局終了 → Next 投票中
    Ended,                                  // 10局完走 or 強制終了
}

pub enum RoomCommand {
    Join { player: PlayerInfo, reply: mpsc::Sender<ServerMessage> },
    Leave { player: PlayerId },
    SubmitAction { player: PlayerId, action: WireAction },
    NextHand { player: PlayerId },
    StartMatch { by: PlayerId },
    AbortHand { by: PlayerId },
    EndMatch { by: PlayerId },
    Disconnect { player: PlayerId },
    Reconnect { player: PlayerId, reply: mpsc::Sender<ServerMessage> },
    /// タイマー満了通知(§6.4 の T1〜T4 で使う)
    Tick,
}

impl Room {
    pub async fn run(mut self) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    if let Err(e) = self.handle(cmd).await {
                        tracing::warn!(?e, "room error");
                    }
                }
                _ = tick.tick() => {
                    self.handle_tick().await;
                }
            }
            if matches!(self.state, RoomPhase::Ended) && self.listeners.is_empty() { break; }
        }
    }
}
```

### 6.1.1 席の決め方(A2)

`StartMatch` 時に、入室済みプレイヤー(必ず5人)を **`rand::rng()` でシャッフル** して seat 0〜4 に
割り当てる。割り当て結果は `MatchStarted { seats: Vec<SeatAssignment> }` で全員に通知。
セット中は固定、再接続後も同じ seat で復帰。

```rust
fn assign_seats_randomly(players: Vec<PlayerInfo>, rng: &mut impl rand::Rng) -> Vec<SeatedPlayer> {
    let mut shuffled = players;
    shuffled.shuffle(rng);
    shuffled.into_iter().map(|p| SeatedPlayer { ... }).collect()
}
```

### 6.1.2 Match の進行(A3, A4)

- `StartMatch` を **ホストが** 発火 → `Match::new` で 1局目開始
- 各局終了で `RoomPhase::BetweenHands` に遷移、`HandEnded` を全員に送信
- 各プレイヤーが `NextHand` を送ると `next_ready` に seat を追加、`NextHandReady` で進捗ブロードキャスト
- **5人全員が押した時点で** `Match::start_next_hand()` を呼んで次局へ(A3 (ii))
- 切断中の人がいると進まない(離席復帰待ち or ホストが `AbortHand` で当該局を捨てる)
- 10局完走で `RoomPhase::Ended` + `MatchEnded { final_scores: Match::cumulative_scores() }`

### 6.2 Hub(全部屋管理)

```rust
// hub.rs
pub struct Hub {
    rooms: DashMap<RoomId, mpsc::Sender<RoomCommand>>,
    cfg: Arc<Config>,
}

impl Hub {
    pub fn create_room(&self, host: PlayerId) -> RoomId { ... }
    pub fn join(&self, room_id: &RoomId, player: PlayerInfo) -> Result<RoomHandle, JoinError> { ... }
    pub fn list_rooms(&self) -> Vec<RoomSummary> { ... }
}
```

- `Hub` は `Arc<Hub>` として axum の `State` に入れる
- 部屋IDは **8文字のランダム英数字**(例: `b3K9mP2x`)。混同しやすい文字(`0/O/1/l`)は除外
- 部屋作成は CF Access を通った任意のユーザができる(B7)
- 1部屋 5人(A1)を超える Join は `RoomFull` でエラー(v1 では観戦も拒否、A6)

### 6.3 接続(WS)から部屋までの橋渡し

```
[axum WS handler]                [Hub]                  [Room actor]
       │                            │                        │
       │ user authenticated         │                        │
       ├──── join(room_id) ────────▶│                        │
       │                            ├─ command via mpsc ────▶│
       │                            │                        │
       │◀── ServerMessage stream ───┤◀── reply mpsc ─────────┤
       │ (each user has own rx)
```

WS ハンドラ内で2つのタスクを spawn:

1. **読み取りタスク**: クライアントからの WS フレーム → デコード → `RoomCommand::Action` を Hub/Room に送信
2. **書き込みタスク**: Room から自分宛の `ServerMessage` を受信 → エンコード → WS 送信

両方とも `select!` でキャンセル/切断を監視。

### 6.4 切断と再接続(A5: 4段階タイムアウト、すべて env で設定可能)

接続断は即時退出ではなく、段階的に処理する。各段階のしきい値はデフォルト値を持ち、
`napo.env` で上書き可能(§9.1)。

| 段階 | env 変数 | デフォルト | 動作 |
|---|---|---|---|
| **T1** 切断検知 | `NAPO_T1_DETECT_MS` | `60000` (60s) | WS Ping 無応答 → セッションを切断、`PlayerConnState::Suspended` に遷移 |
| **T2** 離席通知 | `NAPO_T2_NOTIFY_MS` | `5000` (5s) | T1 後この時間が経つと他プレイヤーに `PlayerDisconnected` をブロードキャスト(短時間の瞬断は通知しない) |
| **T3** Hand 中断可能化 | `NAPO_T3_ABORT_MS` | `300000` (5min) | T1 後この時間が経つとホストが `AbortHand` を選べるようになる(押すと当該局を捨てて次の局へ) |
| **T4** Match 強制終了可能化 | `NAPO_T4_END_MS` | `600000` (10min) | T1 後この時間が経つとホストが `EndMatch` を選べる(セット中断、累積スコアでそのまま終了) |

```rust
// room.rs(タイマー処理の概念)
fn handle_tick(&mut self) {
    let now = Instant::now();
    for seat in self.suspended_seats() {
        let since = now - self.seats[seat].as_ref().unwrap().suspended_since();
        if since > self.cfg.t2 && !self.notified_disconnect.contains(&seat) {
            self.broadcast(ServerMessage::PlayerDisconnected { seat, since_ms: since.as_millis() as u64 });
            self.notified_disconnect.insert(seat);
        }
        // T3 / T4 は「ホスト UI のボタン有効化」用フラグなので server からの通知は不要
        // (ホストが押した時点で `AbortHand` / `EndMatch` を発火、server 側で「経過時間 ≥ T3/T4」を検証)
    }
}
```

#### 再接続

同じ `PlayerId`(JWT の `email` から導出)で再接続すると、**同じ seat に復帰**する。
Room は `PlayerConnState::Online` に戻し、最新の `RoomState` + `Observation` を新セッションに送る。
他プレイヤーには `PlayerReconnected { seat }` をブロードキャスト。

#### 進行ブロック

切断中の人がいる間、Match の進行はブロックされる(その人の番が回ってきても `current_player` のまま停止)。
ホストが `AbortHand`(T3 後)を発火すれば、当該局を捨てて `start_next_hand` で先に進める。

### 6.5 部屋のライフサイクル

| イベント | 動作 |
|---|---|
| 部屋作成 | Hub に登録、`RoomPhase::Lobby` |
| 5人入室 + ホスト StartMatch | 席ランダム配置、`RoomPhase::Playing` |
| 1局終了 | `RoomPhase::BetweenHands`、Next 投票開始 |
| 全員 NextHand | `Match::start_next_hand()` で次局、`RoomPhase::Playing` に戻る |
| 10局完走 | `RoomPhase::Ended`、`MatchEnded` ブロードキャスト |
| ホスト EndMatch(T4 後) | `RoomPhase::Ended` |
| 全員退室 | 即座に削除、関連リソース解放 |
| 全員切断 30分 | 削除(SQLite に最終状態を保存してから) |
| サーバ再起動 | 全部屋消える(進行中の Match は失われる) |

v1 では **「サーバ再起動で全部屋消える」** で十分。スナップショット復元は後回し
(`napo-core::Match` は serde feature で JSON 化可能なので、必要になったら DB に保存して復元できる)。

---

## 7. 認証(Cloudflare Access JWT)

### 7.1 全体像

Cloudflare Access が前段で Google ログインを完了させているため、サーバに到達する
リクエストには `Cf-Access-Jwt-Assertion` ヘッダが付く。サーバ側ではこの JWT を検証して、

- 「Cloudflare を通っていないリクエスト」を弾く(VPS の直 IP 攻撃の二重防御)
- ユーザの email を取得して `PlayerId` にマッピング

### 7.2 検証ロジック

```rust
// auth.rs(骨格)
pub struct Verifier {
    team_domain: String,              // 例: "tedsum.cloudflareaccess.com"
    aud: String,                      // Application Audience tag
    jwks: Arc<RwLock<Option<JwkSet>>>,
}

impl Verifier {
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let url = format!("https://{}/cdn-cgi/access/certs", self.team_domain);
        let jwks: JwkSet = reqwest::get(&url).await?.json().await?;
        *self.jwks.write().await = Some(jwks);
        Ok(())
    }

    pub async fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        let header = decode_header(token)?;
        let kid = header.kid.ok_or(AuthError::MissingKid)?;

        let jwks = self.jwks.read().await;
        let jwks = jwks.as_ref().ok_or(AuthError::JwksNotLoaded)?;
        let jwk = jwks.find(&kid).ok_or(AuthError::UnknownKid)?;
        let key = DecodingKey::from_jwk(jwk)?;

        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&self.aud]);
        validation.set_issuer(&[format!("https://{}", self.team_domain)]);

        let data = decode::<Claims>(token, &key, &validation)?;
        Ok(data.claims)
    }
}

#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub aud: Vec<String>,
    pub exp: usize,
}
```

### 7.3 起動時とミドルウェアでの組み込み

```rust
// main.rs(抜粋)
let verifier = Arc::new(Verifier::new(&cfg.cf_team_domain, &cfg.cf_aud));
verifier.refresh().await?;
{
    let v = verifier.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let _ = v.refresh().await;
        }
    });
}

let app = Router::new()
    .route("/healthz", get(http::healthz))
    .route("/ws", get(ws::ws_handler))
    .route("/api/rooms", get(http::list_rooms))
    .layer(middleware::from_fn_with_state(state.clone(), require_access));
```

### 7.4 ローカル開発時

ローカルでは Cloudflare Access が前段にいないので JWT ヘッダが来ない。`cfg.dev_mode = true`
のときだけ「ヘッダ無くても固定 email でログイン扱い」にする。本番では `false` 固定。

```rust
if cfg.dev_mode {
    return Ok(Claims { email: "dev@local".into(), ..Default::default() });
}
```

### 7.5 PlayerId と email の関係

- JWT の `email` を主キーにユーザを SQLite に保存。
- `PlayerId` はサーバ内部で発行する UUID または email 自体。クライアントには内部 ID を見せず、表示名(handle)のみ見せる。
- 初回ログイン時に「表示名を決めてください」画面でハンドルを登録する流れにする。

---

## 8. データ永続化(SQLite)

### 8.1 何を永続化するか / しないか

| 種類 | 永続化 | 理由 |
|---|---|---|
| ユーザ(email, handle, 登録日時) | する | 再ログイン時の継続 |
| ゲーム履歴(誰が勝った、得点) | する | 後で集計したくなる |
| ゲーム状態のスナップショット | **任意** | 切断復帰のため(最初は省略可) |
| 進行中の部屋一覧 | しない(オンメモリ) | 再起動時に消えてOK |
| WebSocket セッション | しない | 揮発で十分 |

### 8.2 スキーマ例

```sql
CREATE TABLE IF NOT EXISTS users (
    email      TEXT PRIMARY KEY,
    handle     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS games (
    id          TEXT PRIMARY KEY,           -- UUID
    room_id     TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER,
    napoleon    TEXT,                       -- email
    adjutant    TEXT,
    bid_value   INTEGER,
    bid_suit    TEXT,
    napoleon_won INTEGER                    -- 1=勝ち, 0=負け, NULL=進行中
);

CREATE TABLE IF NOT EXISTS game_players (
    game_id  TEXT NOT NULL,
    email    TEXT NOT NULL,
    seat     INTEGER NOT NULL,
    PRIMARY KEY (game_id, email)
);

CREATE TABLE IF NOT EXISTS game_snapshots (
    game_id    TEXT PRIMARY KEY,
    snapshot   BLOB NOT NULL,             -- bincode などで napo-core::Game をシリアライズ
    updated_at INTEGER NOT NULL
);
```

### 8.3 接続戦略

- SQLite は単一プロセスから単一ファイルにアクセスする前提。
- 接続を `Arc<Mutex<Connection>>` で持つか、`r2d2_sqlite` で軽量プールする。
- 書き込みを直列化したいときは「DB タスク」を 1本立てて、他から MPSC で書き込みコマンドを投げる構成にすると分かりやすい。
- WAL モードを有効化(`PRAGMA journal_mode=WAL`)、`synchronous=NORMAL` で十分。

### 8.4 バックアップ

サーバ側では実装不要。VPS 側の cron で `sqlite3 .backup` を打つ運用にする。

---

## 9. 設定と起動

### 9.1 環境変数

```ini
# --- 必須 ---
NAPO_BIND=127.0.0.1:8080
NAPO_DB=/var/lib/napo/game.db
NAPO_STATIC=/opt/napo/static
NAPO_CF_TEAM=tedsum.cloudflareaccess.com
NAPO_CF_AUD=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

# --- 一般 ---
NAPO_DEV_MODE=false
NAPO_LOG=info
NAPO_MAX_ROOMS=64

# --- セット構成(A4) ---
NAPO_HANDS_PER_MATCH=10            # 1セット = 10局

# --- 切断/離席タイマー(A5、すべてミリ秒) ---
NAPO_T1_DETECT_MS=60000            # WS Ping 無応答→切断判定 (60s)
NAPO_T2_NOTIFY_MS=5000             # 他プレイヤーに「切断中」を通知するまで (5s)
NAPO_T3_ABORT_MS=300000            # ホストが AbortHand を選べるまで (5min)
NAPO_T4_END_MS=600000              # ホストが EndMatch を選べるまで (10min)

# --- 安全性(§10) ---
NAPO_WS_MAX_MSG_BYTES=8192         # 1メッセージ 8KB
NAPO_WS_RATE_LIMIT_PER_SEC=10      # 1接続 10msg/s、バーストは 20 まで
```

> プレイ人数(5人)は v1 では env で変更できない(A1 / B5)。
> ハードコードして `Rules::default()` に固定。

### 9.2 main.rs の流れ

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cfg = Config::from_env()?;
    let db = Db::open(&cfg.db_path).await?;
    db.migrate().await?;

    let verifier = Arc::new(Verifier::new(&cfg.cf_team, &cfg.cf_aud));
    verifier.refresh().await?;
    spawn_jwks_refresher(verifier.clone());

    let hub = Arc::new(Hub::new());

    let state = AppState { cfg: cfg.clone(), db, hub, verifier };

    let app = Router::new()
        .route("/healthz", get(http::healthz))
        .route("/ws",      get(ws::ws_handler))
        .nest_service("/", ServeDir::new(&cfg.static_dir))
        .layer(middleware::from_fn_with_state(state.clone(), auth::require_access))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!(bind = %cfg.bind, "starting napo-online");
    axum::serve(listener, app).await?;
    Ok(())
}
```

### 9.3 systemd ユニット

`/etc/systemd/system/napo.service`(VPS 側):

```ini
[Unit]
Description=Napo Online Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=napo
Group=napo
WorkingDirectory=/opt/napo
ExecStart=/opt/napo/napo-online
Restart=on-failure
RestartSec=3s

EnvironmentFile=/etc/napo/napo.env

# リソース上限
MemoryMax=400M
MemoryHigh=300M
TasksMax=300
CPUQuota=80%
LimitNOFILE=4096

# ハードニング
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ReadWritePaths=/var/lib/napo
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
LockPersonality=true
RestrictRealtime=true

[Install]
WantedBy=multi-user.target
```

`/etc/napo/napo.env` に `NAPO_*` を書く。`chmod 640 root:napo` を推奨。

---

## 10. 安全性(アプリ層でやること)

VPS の ufw / Cloudflare 側で多くのケースは弾けるが、アプリ層でも以下を実装する。

| 対策 | 実装 |
|---|---|
| WS メッセージサイズ上限 | axum の `WebSocket` で max_message_size 設定 |
| 接続数上限(全体) | Hub が同時接続数を管理、超過時は 429 で拒否 |
| 部屋ごとの接続数上限 | Room 側で 5接続超なら Reject |
| 1接続のレート制限 | トークンバケット、超過したら警告→切断 |
| 入力バリデーション | `serde` で型を絞る + 値域チェック(`bid_value` の範囲等) |
| ログにメアド原文を出さない | email は SHA-256 短縮ハッシュなどでログに記録 |
| パニックでサーバ落とさない | `tokio::spawn` 配下のタスクは必ず `try` でラップ |
| 部屋IDの推測対策 | 8文字以上のランダム + IPごとの試行レート制限 |

---

## 11. ロギング・観測

最小構成:

```rust
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,napo_online=debug".into())
        )
        .with_target(true)
        .json()
        .init();
}
```

ログに乗せるべきフィールド:

- `request_id`(WS は接続単位 ID)
- `email_hash`(平文は NG)
- `room_id`、`game_id`
- アクション種別とサイズ(中身は出さない、デバッグログにのみ)

`/healthz` は LBや UptimeRobot 用に常時 200 を返す簡素なエンドポイント。
`/metrics` は無くてよい(身内サーバ規模)。

---

## 12. テスト戦略

| 種類 | 何をテストするか | ツール |
|---|---|---|
| ユニット(core) | カードロジック、競り、トリック判定、視点別ビュー | `#[test]` (napo-core 側) |
| ユニット(server) | プロトコル enum の serde 往復、設定パース、ID生成 | `#[test]` |
| 統合(server) | Hub に 5接続立てて 1ゲーム回す | `tokio::test` + 自前 WSクライアント or `tungstenite` |
| エンドツーエンド | 実 VPS に Playwright などで | (任意・後回し可) |

`napo-core` のテストはライブラリ側で完結させる。
サーバ側は **「ライブラリを正しく呼んで、正しく送受信する」** ことだけテストすればよい。

---

## 13. デプロイ運用

### 13.1 ビルド〜配布(クロスコンパイル + scp 方式 / 推奨)

ローカルで:
```bash
# 初回のみ
rustup target add x86_64-unknown-linux-musl
sudo apt install musl-tools     # macOS なら brew install FiloSottile/musl-cross/musl-cross

# napo-core を最新に固定したい場合
cargo update -p napo-core

# クロスコンパイル
cargo build --release --target x86_64-unknown-linux-musl

# VPS へ転送
scp -P 52200 target/x86_64-unknown-linux-musl/release/napo-online \
    ubuntu@napo.<your-domain>:/tmp/
```

VPS 側で:
```bash
sudo install -m 755 -o napo -g napo /tmp/napo-online /opt/napo/
sudo systemctl restart napo
sudo journalctl -u napo -f
```

この方式なら **VPS に napo-core の Deploy key を置く必要がない**(バイナリにはソース不要)。

### 13.2 Justfile による自動化

```make
deploy:
    cargo build --release --target x86_64-unknown-linux-musl
    scp -P 52200 target/x86_64-unknown-linux-musl/release/napo-online \
        ubuntu@napo.<your-domain>:/tmp/
    ssh -p 52200 ubuntu@napo.<your-domain> '\
        sudo install -m 755 -o napo -g napo \
            /tmp/napo-online /opt/napo/napo-online.new && \
        sudo mv /opt/napo/napo-online /opt/napo/napo-online.prev && \
        sudo mv /opt/napo/napo-online.new /opt/napo/napo-online && \
        sudo systemctl restart napo \
    '

logs:
    ssh -p 52200 ubuntu@napo.<your-domain> 'sudo journalctl -u napo -f'

rollback:
    ssh -p 52200 ubuntu@napo.<your-domain> '\
        sudo mv /opt/napo/napo-online /opt/napo/napo-online.broken && \
        sudo mv /opt/napo/napo-online.prev /opt/napo/napo-online && \
        sudo systemctl restart napo \
    '
```

### 13.3 リリース手順

1. 両リポジトリで `cargo test`
2. `napo-core` を tag 切って push(必要なら `Cargo.toml` の `tag` を更新)
3. `napo-online` で `cargo update -p napo-core` → コミット
4. `mise run deploy`
5. ブラウザで `https://napo.<your-domain>` 動作確認
6. 問題があれば `mise run deploy:rollback`

---

## 14. 開発ロードマップ

```
Phase 0: napo-core 安定化【完了】
  ├ Card / Suit / Rank / Deck / Rules
  ├ Game / Action / Phase / Observation
  ├ Match(複数局・累積スコア・bidder 回転)
  ├ serde feature と SCHEMA_VERSION = 1
  └ examples/ + tests/ で API 動作確認

Phase 1: napo-online 基盤(1〜2日)
  ├ cargo init --bin、Cargo.toml(napo-core を [patch] でローカル参照、A10)
  ├ mise.toml(napo-core から拝借)
  ├ Config::from_env()、tracing JSON 初期化
  ├ axum で GET /healthz、起動確認
  ├ auth::Verifier(CF Access JWT 検証 + JWKS キャッシュ + 1h refresh)
  └ DEV_MODE=true で固定 email バイパス

Phase 2: WebSocket とロビー(2〜3日)
  ├ protocol.rs: ClientMessage / ServerMessage / WireAction の型定義
  ├ hub.rs: Hub / RoomId(8文字英数字、混同文字除外)
  ├ room.rs: Room actor 骨格(まだゲームロジック呼ばない)
  ├ ws.rs: アップグレード→2タスク(read/write)+ select! で切断検知
  ├ メッセージサイズ 8KB / レート 10msg/s
  └ CreateRoom / JoinRoom / LeaveRoom と RoomState ブロードキャスト

Phase 3: napo-core 接続(3〜4日)
  ├ game_adapter.rs: GameSession + Observation diff からの PublicEvent 構築
  ├ 席ランダム配置(A2)
  ├ StartMatch → Match::new、SubmitAction → Match::apply
  ├ Event + Observation の二重送信(A8)
  └ 1 局を 5接続で完走させる

Phase 4: Match 進行と永続化(2〜3日)
  ├ Next 投票で局を進める(A3 (ii))
  ├ 10局完走 → MatchEnded(A4)
  ├ db.rs: SQLite + WAL、users / games / game_players スキーマ
  ├ 初回ログイン時の display_name 登録フロー
  ├ 局/セット終了時に履歴保存
  └ deploy/ 一式(Caddyfile / napo.service / napo.env)

Phase 5: 安定化(1〜2週間、並行)
  ├ 切断/再接続(T1〜T4 タイマー、A5)
  ├ AbortHand / EndMatch のホスト操作
  ├ 統合テスト(tokio::test + tokio-tungstenite で 5接続フルセット)
  ├ クロスコンパイル(x86_64-unknown-linux-musl)+ scp 配置確認
  ├ Cloudflare Access の Application 設定
  └ 友達 1〜2人で実プレイ → 本番運用
```

Phase 1〜5 は積み上げ式で、各フェーズの終わりに動くものを1つ仕上げる。
特に Phase 3 終了時点で「5接続で1局回しきれる」状態まで行くのが大きな節目。

---

## 15. 参考リンク

- axum: https://docs.rs/axum/
- tokio: https://docs.rs/tokio/
- Cloudflare Access JWT: https://developers.cloudflare.com/cloudflare-one/identity/authorization-cookie/validating-json/
- Caddy reverse proxy: https://caddyserver.com/docs/quick-starts/reverse-proxy
- SQLite WAL: https://www.sqlite.org/wal.html
- Cargo Git dependencies: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories
- Cargo `[patch]`: https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html

---

## 16. 設計判断の決定一覧

### 16.1 v1 で確定済み

| ID | 項目 | 決定 |
|---|---|---|
| **A1** | プレイ人数 | 5人固定。`Rules::default()` を使い、env で変更不可 |
| **A2** | 席の決め方 | `StartMatch` 時に入室済み 5人を `rand` でシャッフルして seat 0〜4 に割り当て。セット中固定、再接続も同 seat |
| **A3** | Match 進行の主導権 | 各局終了後、全員が `NextHand` を押すと次局へ((ii))。離席があると進まない |
| **A4** | Match の終わり方 | **10局1セット**で自動終了。`NAPO_HANDS_PER_MATCH` で env 上書き可 |
| **A5** | 切断/離席タイマー | T1=60s / T2=5s / T3=5min / T4=10min、すべて `NAPO_T*_MS` で env 上書き可。AI 代行は無し、ホストの `AbortHand` / `EndMatch` で進める |
| **A6** | 観戦機能 | v1 では非対応。protocol/Room の構造に拡張余地は残す(`Observation::spectator()` 相当を v2 で `napo-core` に追加予定) |
| **A7** | 配り直し通知 | `PublicEvent::HandRedealt` + 各人に新 `Observation` を再送 |
| **A8** | イベント vs スナップショット | **両方送る**。`PublicEvent`(差分・アニメ用)+ `ServerMessage::Observation`(各人に最新ビュー、これが正)を二重配信 |
| **A9** | wire の Card 表現 | `"SA"` 1文字スート + ランク英字。`napo-core::Card::FromStr/Display` に準拠 |
| **A10** | napo-core 依存方式 | 開発中は `[patch]` でローカル `../napo-core`、リリース時は git **tag 固定**(branch 追従しない) |
| **B1** | 永続化フォーマット | `napo_core::Match` を `serde` feature で JSON 直列化。`SCHEMA_VERSION` で互換性チェック |
| **B7** | 部屋作成権限 | CF Access を通った任意のユーザが作れる。招待制は Cloudflare Access の Emails policy で完結 |
| **B8** | ハウスルール | `Rules::default()` の標準ローカルルール(マイティ・正J・裏J・よろめき・same2 全て有効、ジョーカーなし) |

### 16.2 v2 以降に保留

| ID | 項目 | メモ |
|---|---|---|
| **B2** | 1手の制限時間 | 身内なので不要。必要になったら `tokio::time::timeout` で Room actor に組み込む |
| **B3** | チャット機能 | `protocol` に `Chat` メッセージを追加するだけで実装可。v2 で対応 |
| **B4** | リプレイ機能 | `Game::action_log()` を DB に保存しておけば後付け可能。`Game::step_iter` で再生 |
| **A6** | 観戦 | v1 では拒否。v2 で `Observation::spectator()` 相当を `napo-core` に追加し、Room の listener を player/spectator に分離 |

### 16.3 v1 で対応しないもの

| ID | 項目 |
|---|---|
| **B5** | 4人ナポなど他人数 |
| **B6** | napo-core の OSS 化(当面 private) |

新しい論点が出たらこのセクションに追記する。実装を進めて決定を変えた場合も、ここに記録を残す。