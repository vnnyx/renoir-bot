use std::collections::HashMap;
use std::sync::Arc;

use poise::serenity_prelude::GuildId;
use tokio::sync::RwLock;

use crate::domain::queue::MusicQueue;
use crate::domain::track::Track;

pub type GuildQueues = Arc<RwLock<HashMap<GuildId, MusicQueue>>>;

pub struct QueueService;

impl QueueService {
    pub fn new_guild_queues() -> GuildQueues {
        Arc::new(RwLock::new(HashMap::new()))
    }

    pub async fn add_track(queues: &GuildQueues, guild_id: GuildId, track: Track) {
        let mut map = queues.write().await;
        map.entry(guild_id).or_default().push(track);
    }

    pub async fn skip(queues: &GuildQueues, guild_id: GuildId) -> Option<Track> {
        let mut map = queues.write().await;
        map.get_mut(&guild_id)?.pop()
    }

    pub async fn clear(queues: &GuildQueues, guild_id: GuildId) {
        let mut map = queues.write().await;
        if let Some(queue) = map.get_mut(&guild_id) {
            queue.clear();
        }
    }

    pub async fn list(queues: &GuildQueues, guild_id: GuildId) -> Vec<Track> {
        let map = queues.read().await;
        match map.get(&guild_id) {
            Some(queue) => queue.list().iter().cloned().collect(),
            None => Vec::new(),
        }
    }
}
