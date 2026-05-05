pub mod auth;
pub mod config;
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
use crate::hub::Hub;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub verifier: Option<Arc<Verifier>>,
    pub hub: Hub,
}
