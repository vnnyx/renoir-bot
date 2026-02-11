use std::sync::atomic::Ordering;

use poise::serenity_prelude::GuildId;

use crate::services::queue_service::QueueService;
use crate::{EnqueueCancels, InactivityHandles};
use crate::services::queue_service::GuildQueues;

/// Cancels background enqueue tasks, clears the queue, and stops the inactivity
/// monitor for a guild. Call this whenever the bot disconnects (by command,
/// inactivity, or being kicked).
pub async fn cleanup_guild(
    guild_id: GuildId,
    guild_queues: &GuildQueues,
    enqueue_cancels: &EnqueueCancels,
    inactivity_handles: &InactivityHandles,
) {
    // Cancel all background enqueue tasks
    if let Some(flags) = enqueue_cancels.write().await.remove(&guild_id) {
        for flag in flags {
            flag.store(true, Ordering::Relaxed);
        }
    }

    // Clear track queue
    QueueService::clear(guild_queues, guild_id).await;

    // Cancel inactivity monitor
    if let Some(cancel) = inactivity_handles.write().await.remove(&guild_id) {
        cancel.notify_one();
    }
}
