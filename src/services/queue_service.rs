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

    /// Advances the queue: pops the next track into `current` and returns a clone.
    pub async fn advance(queues: &GuildQueues, guild_id: GuildId) -> Option<Track> {
        let mut map = queues.write().await;
        let queue = map.get_mut(&guild_id)?;
        queue.advance().cloned()
    }

    /// Returns a clone of the currently playing track (read lock only).
    pub async fn current(queues: &GuildQueues, guild_id: GuildId) -> Option<Track> {
        let map = queues.read().await;
        map.get(&guild_id)?.current().cloned()
    }

    /// Takes the currently playing track out of the queue (used for skip messages).
    pub async fn skip(queues: &GuildQueues, guild_id: GuildId) -> Option<Track> {
        let mut map = queues.write().await;
        map.get_mut(&guild_id)?.take_current()
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
