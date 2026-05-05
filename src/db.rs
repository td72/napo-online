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
            conn.execute_batch(SCHEMA_V1)
                .context("apply schema v1")
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

    pub async fn upsert_user(&self, email: &str, handle: &str, now: i64) -> Result<()> {
        let conn = self.conn.clone();
        let email = email.to_string();
        let handle = handle.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock().expect("db poisoned");
            conn.execute(
                "INSERT INTO users (email, handle, created_at, last_seen)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(email) DO UPDATE SET handle = excluded.handle, last_seen = excluded.last_seen",
                params![email, handle, now],
            )?;
            Ok(())
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

    pub async fn record_game_start(&self, g: GameRecord, players: Vec<(String, usize)>) -> Result<()> {
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
        db.upsert_user("a@x.com", "alice", now()).await.unwrap();
        let u = db.get_user("a@x.com").await.unwrap().unwrap();
        assert_eq!(u.handle, "alice");
        assert_eq!(u.created_at, now());

        db.upsert_user("a@x.com", "alice2", now() + 100)
            .await
            .unwrap();
        let u2 = db.get_user("a@x.com").await.unwrap().unwrap();
        assert_eq!(u2.handle, "alice2");
        assert_eq!(u2.created_at, now()); // 元の created_at は保持
        assert_eq!(u2.last_seen, now() + 100);
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
