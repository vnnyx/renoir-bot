use crate::services::error::MusicError;
use crate::services::queue_service::QueueService;
use crate::{Context, Error};

/// Stop playback, clear the queue, and leave the voice channel
#[poise::command(slash_command, guild_only)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird not registered");

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        handler.queue().stop();
    }

    manager
        .leave(guild_id)
        .await
        .map_err(|e| MusicError::JoinError(e.to_string()))?;

    QueueService::clear(&ctx.data().guild_queues, guild_id).await;

    ctx.say("Stopped playback and left the voice channel.").await?;
    Ok(())
}
