use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    email      TEXT PRIMARY KEY,
    handle     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS games (
    id           TEXT PRIMARY KEY,
    room_id      TEXT NOT NULL,
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER,
    napoleon     TEXT,
    adjutant     TEXT,
    bid_value    INTEGER,
    bid_suit     TEXT,
    napoleon_won INTEGER
);

CREATE TABLE IF NOT EXISTS game_players (
    game_id TEXT NOT NULL,
    email   TEXT NOT NULL,
    seat    INTEGER NOT NULL,
    score   INTEGER,
    PRIMARY KEY (game_id, email)
);
"#;

/// users.handle に大小無視の UNIQUE 制約を追加する。
/// SQLite は ALTER TABLE で UNIQUE 追加できないので rename-recreate-copy で対応。
const SCHEMA_V2_HANDLE_UNIQUE: &str = r#"
CREATE TABLE users_new (
    email      TEXT PRIMARY KEY,
    handle     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL,
    UNIQUE(handle COLLATE NOCASE)
);
INSERT INTO users_new SELECT email, handle, created_at, last_seen FROM users;
DROP TABLE users;
ALTER TABLE users_new RENAME TO users;
"#;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct User {
    pub email: String,
    pub handle: String,
    pub created_at: i64,
    pub last_seen: i64,
}

#[derive(Debug, Clone)]
pub struct GameRecord {
    pub id: String,
    pub room_id: String,
    pub started_at: i64,
    pub napoleon: Option<String>,
    pub adjutant: Option<String>,
    pub bid_value: Option<i32>,
    pub bid_suit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameOutcome {
    pub ended_at: i64,
    pub napoleon_won: Option<bool>,
    pub seat_scores: Vec<(String, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertResult {
    Ok,
    /// 別 email が同じ handle (大小無視) を使っているため変更できない
    HandleTaken,
}

#[derive(Debug, Clone)]
pub struct GamePlayerRow {
    pub email: String,
    pub handle: Option<String>,
    pub seat: usize,
    pub score: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct GameSummary {
    pub id: String,
    pub room_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub napoleon_email: Option<String>,
    pub napoleon_handle: Option<String>,
    pub adjutant_email: Option<String>,
    pub adjutant_handle: Option<String>,
    pub bid_value: Option<i32>,
    pub bid_suit: Option<String>,
    pub napoleon_won: Option<bool>,
    pub players: Vec<GamePlayerRow>,
}

impl Db {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection> {
            let conn = Connection::open(&path)
                .with_context(|| format!("opening sqlite at {}", path.display()))?;
            // SQLite pragmas: WAL でリーダーがブロックされない、NORMAL で fsync 頻度下げる
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            Ok(conn)
        })
        .await??;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 主にテスト用。`:memory:` は spawn_blocking 経由だと別スレッドで作るので
    /// プロセス内でも別 DB になる点に注意。本テストでは `Db` を作って同じ Arc を共有する。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub async fn migrate(&self) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock().expect("db poisoned");
            let current: i64 = conn
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .context("read user_version")?;
            if current < 1 {
                conn.execute_batch(SCHEMA_V1).context("apply schema v1")?;
                conn.pragma_update(None, "user_version", 1)
                    .context("bump user_version to 1")?;
            }
            if current < 2 {
                conn.execute_batch(SCHEMA_V2_HANDLE_UNIQUE)
                    .context("apply schema v2 (handle UNIQUE)")?;
                conn.pragma_update(None, "user_version", 2)
                    .context("bump user_version to 2")?;
            }
            Ok(())
        })
        .await?
    }

    pub async fn get_user(&self, email: &str) -> Result<Option<User>> {
        let conn = self.conn.clone();
        let email = email.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<User>> {
            let conn = conn.lock().expect("db poisoned");
            conn.query_row(
                "SELECT email, handle, created_at, last_seen FROM users WHERE email = ?1",
                params![email],
                |row| {
                    Ok(User {
                        email: row.get(0)?,
                        handle: row.get(1)?,
                        created_at: row.get(2)?,
                        last_seen: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn upsert_user(
        &self,
        email: &str,
        handle: &str,
        now: i64,
    ) -> Result<UpsertResult> {
        let conn = self.conn.clone();
        let email = email.to_string();
        let handle = handle.to_string();
        tokio::task::spawn_blocking(move || -> Result<UpsertResult> {
            let conn = conn.lock().expect("db poisoned");
            // 事前チェック: 別 email が同じ handle (NOCASE) を持っていないか
            let taken_by: Option<String> = conn
                .query_row(
                    "SELECT email FROM users WHERE handle = ?1 COLLATE NOCASE AND email != ?2",
                    params![handle, email],
                    |r| r.get(0),
                )
                .optional()?;
            if taken_by.is_some() {
                return Ok(UpsertResult::HandleTaken);
            }
            conn.execute(
                "INSERT INTO users (email, handle, created_at, last_seen)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(email) DO UPDATE SET handle = excluded.handle, last_seen = excluded.last_seen",
                params![email, handle, now],
            )?;
            Ok(UpsertResult::Ok)
        })
        .await?
    }

    /// 終局済みの局を新しい順に最大 `limit` 件返す。
    /// napoleon / adjutant / 各 player の handle は users テーブルとの LEFT JOIN で解決。
    pub async fn list_recent_games(&self, limit: usize) -> Result<Vec<GameSummary>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<GameSummary>> {
            let conn = conn.lock().expect("db poisoned");

            // 1) ゲーム本体 + napoleon/adjutant の handle
            struct GameRow {
                id: String,
                room_id: String,
                started_at: i64,
                ended_at: Option<i64>,
                napoleon_email: Option<String>,
                napoleon_handle: Option<String>,
                adjutant_email: Option<String>,
                adjutant_handle: Option<String>,
                bid_value_i64: Option<i64>,
                bid_suit: Option<String>,
                napoleon_won: Option<i64>,
            }
            let games: Vec<GameRow> = {
                let mut stmt = conn.prepare(
                    "SELECT g.id, g.room_id, g.started_at, g.ended_at,
                            g.napoleon, un.handle,
                            g.adjutant, ua.handle,
                            g.bid_value, g.bid_suit, g.napoleon_won
                     FROM games g
                     LEFT JOIN users un ON un.email = g.napoleon
                     LEFT JOIN users ua ON ua.email = g.adjutant
                     WHERE g.ended_at IS NOT NULL
                     ORDER BY g.ended_at DESC
                     LIMIT ?1",
                )?;
                stmt.query_map(params![limit as i64], |r| {
                    Ok(GameRow {
                        id: r.get(0)?,
                        room_id: r.get(1)?,
                        started_at: r.get(2)?,
                        ended_at: r.get(3)?,
                        napoleon_email: r.get(4)?,
                        napoleon_handle: r.get(5)?,
                        adjutant_email: r.get(6)?,
                        adjutant_handle: r.get(7)?,
                        bid_value_i64: r.get(8)?,
                        bid_suit: r.get(9)?,
                        napoleon_won: r.get(10)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
            };

            // 2) 各 game について seat 順に players を取得
            let mut summaries = Vec::with_capacity(games.len());
            for GameRow {
                id,
                room_id,
                started_at,
                ended_at,
                napoleon_email,
                napoleon_handle,
                adjutant_email,
                adjutant_handle,
                bid_value_i64,
                bid_suit,
                napoleon_won,
            } in games
            {
                let players: Vec<GamePlayerRow> = {
                    let mut stmt = conn.prepare(
                        "SELECT gp.email, u.handle, gp.seat, gp.score
                         FROM game_players gp
                         LEFT JOIN users u ON u.email = gp.email
                         WHERE gp.game_id = ?1
                         ORDER BY gp.seat",
                    )?;
                    stmt.query_map(params![id], |r| {
                        Ok(GamePlayerRow {
                            email: r.get(0)?,
                            handle: r.get(1)?,
                            seat: r.get::<_, i64>(2)? as usize,
                            score: r.get(3)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?
                };
                summaries.push(GameSummary {
                    id,
                    room_id,
                    started_at,
                    ended_at,
                    napoleon_email,
                    napoleon_handle,
                    adjutant_email,
                    adjutant_handle,
                    bid_value: bid_value_i64.map(|v| v as i32),
                    bid_suit,
                    napoleon_won: napoleon_won.map(|v| v != 0),
                    players,
                });
            }
            Ok(summaries)
        })
        .await?
    }

    pub async fn list_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<User>> {
            let conn = conn.lock().expect("db poisoned");
            let mut stmt = conn.prepare(
                "SELECT email, handle, created_at, last_seen FROM users ORDER BY last_seen DESC",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(User {
                        email: row.get(0)?,
                        handle: row.get(1)?,
                        created_at: row.get(2)?,
                        last_seen: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?
    }

    pub async fn touch_last_seen(&self, email: &str, now: i64) -> Result<()> {
        let conn = self.conn.clone();
        let email = email.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock().expect("db poisoned");
            conn.execute(
                "UPDATE users SET last_seen = ?2 WHERE email = ?1",
                params![email, now],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn record_game_start(
        &self,
        g: GameRecord,
        players: Vec<(String, usize)>,
    ) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = conn.lock().expect("db poisoned");
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO games (id, room_id, started_at, napoleon, adjutant, bid_value, bid_suit)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    g.id,
                    g.room_id,
                    g.started_at,
                    g.napoleon,
                    g.adjutant,
                    g.bid_value,
                    g.bid_suit
                ],
            )?;
            for (email, seat) in &players {
                tx.execute(
                    "INSERT INTO game_players (game_id, email, seat) VALUES (?1, ?2, ?3)",
                    params![g.id, email, *seat as i64],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn record_game_end(&self, game_id: &str, outcome: GameOutcome) -> Result<()> {
        let conn = self.conn.clone();
        let game_id = game_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = conn.lock().expect("db poisoned");
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE games SET ended_at = ?2, napoleon_won = ?3 WHERE id = ?1",
                params![
                    game_id,
                    outcome.ended_at,
                    outcome.napoleon_won.map(|w| w as i64)
                ],
            )?;
            for (email, score) in &outcome.seat_scores {
                tx.execute(
                    "UPDATE game_players SET score = ?3 WHERE game_id = ?1 AND email = ?2",
                    params![game_id, email, score],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        1_700_000_000
    }

    async fn fresh_db() -> Db {
        let db = Db::open_in_memory().expect("open");
        db.migrate().await.expect("migrate");
        db
    }

    #[tokio::test]
    async fn migrate_creates_tables() {
        let db = fresh_db().await;
        // users テーブルが空でクエリできること
        assert!(db.get_user("nobody@example.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_user_inserts_then_updates_handle() {
        let db = fresh_db().await;
        let r1 = db.upsert_user("a@x.com", "alice", now()).await.unwrap();
        assert_eq!(r1, UpsertResult::Ok);
        let u = db.get_user("a@x.com").await.unwrap().unwrap();
        assert_eq!(u.handle, "alice");
        assert_eq!(u.created_at, now());

        let r2 = db
            .upsert_user("a@x.com", "alice2", now() + 100)
            .await
            .unwrap();
        assert_eq!(r2, UpsertResult::Ok);
        let u2 = db.get_user("a@x.com").await.unwrap().unwrap();
        assert_eq!(u2.handle, "alice2");
        assert_eq!(u2.created_at, now()); // 元の created_at は保持
        assert_eq!(u2.last_seen, now() + 100);
    }

    #[tokio::test]
    async fn upsert_user_rejects_duplicate_handle_case_insensitive() {
        let db = fresh_db().await;
        db.upsert_user("a@x.com", "Alice", now()).await.unwrap();
        let r = db
            .upsert_user("b@x.com", "alice", now())
            .await
            .unwrap();
        assert_eq!(r, UpsertResult::HandleTaken);
        // a@x.com は影響なし
        assert_eq!(db.get_user("a@x.com").await.unwrap().unwrap().handle, "Alice");
        // b@x.com は登録されていない
        assert!(db.get_user("b@x.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_user_allows_same_email_to_keep_their_handle() {
        let db = fresh_db().await;
        db.upsert_user("a@x.com", "Alice", now()).await.unwrap();
        // 同じ email の更新は弾かれない(自分の handle)
        let r = db.upsert_user("a@x.com", "Alice", now() + 1).await.unwrap();
        assert_eq!(r, UpsertResult::Ok);
    }

    #[tokio::test]
    async fn list_recent_games_returns_finished_games_with_handles_and_players() {
        let db = fresh_db().await;
        // ユーザを先に登録(handle が join で拾われる)
        db.upsert_user("a@x.com", "alice", 100).await.unwrap();
        db.upsert_user("b@x.com", "bob", 100).await.unwrap();
        db.upsert_user("c@x.com", "carol", 100).await.unwrap();

        let g = GameRecord {
            id: "game-1".into(),
            room_id: "ROOM1".into(),
            started_at: 1000,
            napoleon: Some("a@x.com".into()),
            adjutant: Some("c@x.com".into()),
            bid_value: Some(13),
            bid_suit: Some("S".into()),
        };
        db.record_game_start(
            g,
            vec![
                ("a@x.com".into(), 0),
                ("b@x.com".into(), 1),
                ("c@x.com".into(), 2),
            ],
        )
        .await
        .unwrap();
        db.record_game_end(
            "game-1",
            GameOutcome {
                ended_at: 2000,
                napoleon_won: Some(true),
                seat_scores: vec![
                    ("a@x.com".into(), 4),
                    ("b@x.com".into(), -2),
                    ("c@x.com".into(), -2),
                ],
            },
        )
        .await
        .unwrap();

        let games = db.list_recent_games(10).await.unwrap();
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!(g.id, "game-1");
        assert_eq!(g.napoleon_handle.as_deref(), Some("alice"));
        assert_eq!(g.adjutant_handle.as_deref(), Some("carol"));
        assert_eq!(g.bid_value, Some(13));
        assert_eq!(g.napoleon_won, Some(true));
        assert_eq!(g.players.len(), 3);
        assert_eq!(g.players[0].handle.as_deref(), Some("alice"));
        assert_eq!(g.players[0].score, Some(4));
        assert_eq!(g.players[2].handle.as_deref(), Some("carol"));
    }

    #[tokio::test]
    async fn list_recent_games_excludes_unfinished_and_respects_limit_and_order() {
        let db = fresh_db().await;
        db.upsert_user("a@x.com", "alice", 100).await.unwrap();
        // 終了済み 3件 + 進行中 1件
        for (id, ended_at) in [("g1", Some(100)), ("g2", Some(300)), ("g3", Some(200))] {
            db.record_game_start(
                GameRecord {
                    id: id.into(),
                    room_id: "R".into(),
                    started_at: 0,
                    napoleon: Some("a@x.com".into()),
                    adjutant: None,
                    bid_value: Some(13),
                    bid_suit: Some("S".into()),
                },
                vec![("a@x.com".into(), 0)],
            )
            .await
            .unwrap();
            if let Some(end) = ended_at {
                db.record_game_end(
                    id,
                    GameOutcome {
                        ended_at: end,
                        napoleon_won: Some(true),
                        seat_scores: vec![("a@x.com".into(), 1)],
                    },
                )
                .await
                .unwrap();
            }
        }
        // 進行中
        db.record_game_start(
            GameRecord {
                id: "g4".into(),
                room_id: "R".into(),
                started_at: 0,
                napoleon: Some("a@x.com".into()),
                adjutant: None,
                bid_value: Some(13),
                bid_suit: Some("S".into()),
            },
            vec![("a@x.com".into(), 0)],
        )
        .await
        .unwrap();

        let g = db.list_recent_games(2).await.unwrap();
        assert_eq!(g.len(), 2);
        // ended_at DESC: g2 (300), g3 (200)
        assert_eq!(g[0].id, "g2");
        assert_eq!(g[1].id, "g3");
    }

    #[tokio::test]
    async fn list_users_returns_all_ordered_by_last_seen_desc() {
        let db = fresh_db().await;
        db.upsert_user("a@x.com", "alice", 100).await.unwrap();
        db.upsert_user("b@x.com", "bob", 200).await.unwrap();
        db.upsert_user("c@x.com", "carol", 150).await.unwrap();
        let users = db.list_users().await.unwrap();
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].email, "b@x.com");
        assert_eq!(users[1].email, "c@x.com");
        assert_eq!(users[2].email, "a@x.com");
    }

    #[tokio::test]
    async fn touch_last_seen_only_updates_timestamp() {
        let db = fresh_db().await;
        db.upsert_user("a@x.com", "alice", now()).await.unwrap();
        db.touch_last_seen("a@x.com", now() + 50).await.unwrap();
        let u = db.get_user("a@x.com").await.unwrap().unwrap();
        assert_eq!(u.handle, "alice");
        assert_eq!(u.last_seen, now() + 50);
    }

    #[tokio::test]
    async fn record_game_start_persists_game_and_players() {
        let db = fresh_db().await;
        let g = GameRecord {
            id: "g1".into(),
            room_id: "ABCD1234".into(),
            started_at: now(),
            napoleon: Some("alice@x.com".into()),
            adjutant: Some("bob@x.com".into()),
            bid_value: Some(13),
            bid_suit: Some("S".into()),
        };
        let players = vec![
            ("alice@x.com".to_string(), 0),
            ("bob@x.com".to_string(), 1),
            ("carol@x.com".to_string(), 2),
            ("dave@x.com".to_string(), 3),
            ("eve@x.com".to_string(), 4),
        ];
        db.record_game_start(g, players).await.unwrap();
    }

    #[tokio::test]
    async fn record_game_end_updates_scores() {
        let db = fresh_db().await;
        db.record_game_start(
            GameRecord {
                id: "g2".into(),
                room_id: "R".into(),
                started_at: now(),
                napoleon: Some("a".into()),
                adjutant: None,
                bid_value: Some(13),
                bid_suit: Some("S".into()),
            },
            vec![("a".to_string(), 0), ("b".to_string(), 1)],
        )
        .await
        .unwrap();
        db.record_game_end(
            "g2",
            GameOutcome {
                ended_at: now() + 600,
                napoleon_won: Some(false),
                seat_scores: vec![("a".to_string(), -2), ("b".to_string(), 1)],
            },
        )
        .await
        .unwrap();
    }
}
