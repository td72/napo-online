use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
#[allow(dead_code)] // 各フィールドは Phase 1〜5 で順に参照される
pub struct Config {
    pub bind: SocketAddr,
    pub log: String,
    pub dev_mode: bool,

    pub static_dir: Option<String>,
    pub db_path: String,

    pub cf_team: Option<String>,
    pub cf_aud: Option<String>,

    pub max_rooms: usize,
    pub hands_per_match: u32,

    pub t1_detect: Duration,
    pub t2_notify: Duration,
    pub t3_abort: Duration,
    pub t4_end: Duration,

    pub ws_max_msg_bytes: usize,
    pub ws_rate_limit_per_sec: u32,
}

impl Config {
    /// テスト用のデフォルト Config。dev_mode、in-memory DB、短いタイマー。
    /// 統合テストや bench で `AppState` を組み立てる際の起点。
    pub fn for_test() -> Self {
        Self {
            bind: "127.0.0.1:0".parse().unwrap(),
            log: "info".into(),
            dev_mode: true,
            static_dir: None,
            db_path: ":memory:".into(),
            cf_team: None,
            cf_aud: None,
            max_rooms: 8,
            hands_per_match: 2,
            t1_detect: Duration::from_secs(60),
            t2_notify: Duration::from_millis(50),
            t3_abort: Duration::from_millis(200),
            t4_end: Duration::from_millis(500),
            ws_max_msg_bytes: 8192,
            ws_rate_limit_per_sec: 100,
        }
    }

    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let dev_mode = parse_or("NAPO_DEV_MODE", false)?;
        let cf_team = std::env::var("NAPO_CF_TEAM").ok();
        let cf_aud = std::env::var("NAPO_CF_AUD").ok();

        if !dev_mode && (cf_team.is_none() || cf_aud.is_none()) {
            anyhow::bail!(
                "NAPO_CF_TEAM and NAPO_CF_AUD are required when NAPO_DEV_MODE is not true"
            );
        }

        Ok(Self {
            bind: env_or("NAPO_BIND", "127.0.0.1:8080")?
                .parse()
                .context("NAPO_BIND must be a socket address like 127.0.0.1:8080")?,
            log: env_or("NAPO_LOG", "info,napo_online=debug")?,
            dev_mode,
            static_dir: std::env::var("NAPO_STATIC").ok(),
            db_path: env_or("NAPO_DB", "./napo.db")?,
            cf_team,
            cf_aud,
            max_rooms: parse_or("NAPO_MAX_ROOMS", 64usize)?,
            hands_per_match: parse_or("NAPO_HANDS_PER_MATCH", 10u32)?,
            t1_detect: ms("NAPO_T1_DETECT_MS", 60_000)?,
            t2_notify: ms("NAPO_T2_NOTIFY_MS", 5_000)?,
            t3_abort: ms("NAPO_T3_ABORT_MS", 300_000)?,
            t4_end: ms("NAPO_T4_END_MS", 600_000)?,
            ws_max_msg_bytes: parse_or("NAPO_WS_MAX_MSG_BYTES", 8_192usize)?,
            ws_rate_limit_per_sec: parse_or("NAPO_WS_RATE_LIMIT_PER_SEC", 10u32)?,
        })
    }
}

fn env_or(key: &str, default: &str) -> Result<String> {
    Ok(std::env::var(key).unwrap_or_else(|_| default.to_string()))
}

fn parse_or<T>(key: &str, default: T) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(s) => s.parse().map_err(|e| anyhow::anyhow!("{key}: {e}")),
        Err(_) => Ok(default),
    }
}

fn ms(key: &str, default_ms: u64) -> Result<Duration> {
    let v: u64 = parse_or(key, default_ms)?;
    Ok(Duration::from_millis(v))
}
