use crate::services::error::MusicError;
use crate::services::queue_service::QueueService;
use crate::{Context, Error};

/// Skip the current track
#[poise::command(slash_command, guild_only)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird not registered");

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        if queue.is_empty() {
            return Err(MusicError::EmptyQueue.into());
        }
        let _ = queue.skip();
    } else {
        return Err(MusicError::EmptyQueue.into());
    }

    let skipped = QueueService::skip(&ctx.data().guild_queues, guild_id).await;

    match skipped {
        Some(track) => ctx.say(format!("Skipped: **{}**", track)).await?,
        None => ctx.say("Skipped current track.").await?,
    };

    Ok(())
}
