use crate::services::cleanup::cleanup_guild;
use crate::services::error::MusicError;
use crate::{Context, Error};

/// Stop playback, clear the queue, and leave the voice channel
#[poise::command(slash_command, guild_only)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;
    ctx.defer().await?;
    let data = ctx.data();

    // Cancel background enqueue tasks FIRST so they stop adding tracks
    cleanup_guild(
        guild_id,
        &data.guild_queues,
        &data.enqueue_cancels,
        &data.inactivity_handles,
    )
    .await;

    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird not registered");

    // Stop the songbird queue (clears any last track that slipped through)
    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        handler.queue().stop();
    }

    manager
        .leave(guild_id)
        .await
        .map_err(|e| MusicError::JoinError(e.to_string()))?;

    ctx.say("Stopped playback and left the voice channel.").await?;
    Ok(())
}
