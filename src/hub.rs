use std::sync::Arc;

use dashmap::DashMap;
use rand::RngExt;
use tokio::sync::mpsc;

use crate::protocol::{PlayerId, RoomId, ServerMessage};
use crate::room::{PlayerInfo, Room, RoomCommand};

const ROOM_ID_LEN: usize = 8;
/// 0 / O / 1 / l / I を除いた可読英数字
const ROOM_ID_CHARSET: &[u8] =
    b"23456789ABCDEFGHJKMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz";
pub const MAX_PLAYERS_PER_ROOM: usize = 5;
const ROOM_CMD_BUFFER: usize = 64;

#[derive(Clone)]
pub struct Hub {
    rooms: Arc<DashMap<RoomId, mpsc::Sender<RoomCommand>>>,
    max_rooms: usize,
    hands_per_match: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateRoomError {
    #[error("max rooms reached")]
    MaxRoomsReached,
}

#[derive(Debug, thiserror::Error)]
pub enum JoinError {
    #[error("room not found")]
    NotFound,
    #[error("room is closed")]
    Closed,
}

#[derive(Clone)]
pub struct RoomHandle {
    pub id: RoomId,
    pub tx: mpsc::Sender<RoomCommand>,
}

impl Hub {
    pub fn new(max_rooms: usize, hands_per_match: u32) -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            max_rooms,
            hands_per_match,
        }
    }

    pub fn create_room(&self, host: PlayerId) -> Result<RoomId, CreateRoomError> {
        if self.rooms.len() >= self.max_rooms {
            return Err(CreateRoomError::MaxRoomsReached);
        }
        let room_id = self.generate_unique_id();
        let (tx, rx) = mpsc::channel(ROOM_CMD_BUFFER);
        let room = Room::new(
            room_id.clone(),
            host,
            rx,
            MAX_PLAYERS_PER_ROOM,
            self.hands_per_match,
        );

        let rooms_for_cleanup = self.rooms.clone();
        let id_for_cleanup = room_id.clone();
        tokio::spawn(async move {
            room.run().await;
            rooms_for_cleanup.remove(&id_for_cleanup);
        });

        self.rooms.insert(room_id.clone(), tx);
        tracing::info!(room = %room_id, "room created");
        Ok(room_id)
    }

    pub async fn join(
        &self,
        room_id: &RoomId,
        info: PlayerInfo,
        reply: mpsc::Sender<ServerMessage>,
    ) -> Result<RoomHandle, JoinError> {
        let tx = self
            .rooms
            .get(room_id)
            .ok_or(JoinError::NotFound)?
            .clone();
        tx.send(RoomCommand::Join { info, reply })
            .await
            .map_err(|_| JoinError::Closed)?;
        Ok(RoomHandle {
            id: room_id.clone(),
            tx,
        })
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    fn generate_unique_id(&self) -> RoomId {
        let mut rng = rand::rng();
        loop {
            let id: String = (0..ROOM_ID_LEN)
                .map(|_| {
                    let idx = rng.random_range(0..ROOM_ID_CHARSET.len());
                    ROOM_ID_CHARSET[idx] as char
                })
                .collect();
            if !self.rooms.contains_key(&id) {
                return id;
            }
        }
    }
}
