use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude::{Cache, ChannelId, CreateMessage, GuildId, Http};
use tokio::sync::Notify;

use crate::services::cleanup::cleanup_guild;
use crate::services::queue_service::GuildQueues;
use crate::{EnqueueCancels, InactivityHandles};

const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Spawns a background task that auto-disconnects the bot after 15 minutes
/// of inactivity (empty queue or alone in the voice channel).
///
/// Returns a `Notify` handle — notify it to cancel the task early (e.g. on `/stop`).
pub fn spawn_inactivity_monitor(
    manager: Arc<songbird::Songbird>,
    guild_id: GuildId,
    voice_channel_id: ChannelId,
    text_channel_id: ChannelId,
    http: Arc<Http>,
    cache: Arc<Cache>,
    guild_queues: GuildQueues,
    inactivity_handles: InactivityHandles,
    enqueue_cancels: EnqueueCancels,
) -> Arc<Notify> {
    let cancel = Arc::new(Notify::new());
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        let mut idle_elapsed = Duration::ZERO;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(CHECK_INTERVAL) => {}
                _ = cancel_clone.notified() => {
                    return;
                }
            }

            let idle = is_idle(&manager, guild_id, voice_channel_id, &cache).await;

            if idle {
                idle_elapsed += CHECK_INTERVAL;
            } else {
                idle_elapsed = Duration::ZERO;
            }

            if idle_elapsed >= INACTIVITY_TIMEOUT {
                if let Some(handler_lock) = manager.get(guild_id) {
                    let handler = handler_lock.lock().await;
                    handler.queue().stop();
                }
                let _ = manager.leave(guild_id).await;

                cleanup_guild(
                    guild_id,
                    &guild_queues,
                    &enqueue_cancels,
                    &inactivity_handles,
                )
                .await;

                let msg = CreateMessage::new()
                    .content("Disconnected due to 15 minutes of inactivity.");
                let _ = text_channel_id.send_message(&http, msg).await;

                return;
            }
        }
    });

    cancel
}

async fn is_idle(
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
    voice_channel_id: ChannelId,
    cache: &Arc<Cache>,
) -> bool {
    // Check if queue is empty (nothing playing)
    let queue_empty = if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        handler.queue().is_empty()
    } else {
        return true;
    };

    if queue_empty {
        return true;
    }

    // Check if bot is alone in the voice channel
    if let Some(guild) = cache.guild(guild_id) {
        let members_in_channel = guild
            .voice_states
            .values()
            .filter(|vs| vs.channel_id == Some(voice_channel_id))
            .count();

        if members_in_channel <= 1 {
            return true;
        }
    }

    false
}
