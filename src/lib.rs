pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod game_adapter;
pub mod http;
pub mod hub;
pub mod protocol;
pub mod room;
pub mod ws;

use std::sync::Arc;

use crate::auth::Verifier;
use crate::config::Config;
use crate::db::Db;
use crate::hub::Hub;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub verifier: Option<Arc<Verifier>>,
    pub hub: Hub,
    pub db: Db,
}

/// 現在の Unix 秒。SystemTime が EPOCH より前(時計巻き戻し等)の場合は 0 を返す。
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
