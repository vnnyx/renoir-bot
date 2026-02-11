use crate::services::error::MusicError;
use crate::services::queue_service::QueueService;
use crate::{Context, Error};

/// Show the current music queue
#[poise::command(slash_command, guild_only)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    let tracks = QueueService::list(&ctx.data().guild_queues, guild_id).await;

    if tracks.is_empty() {
        return Err(MusicError::EmptyQueue.into());
    }

    let mut msg = String::from("**Music Queue:**\n");
    for (i, track) in tracks.iter().enumerate() {
        if i == 0 {
            msg.push_str(&format!("Now playing: {} **{}** - {}\n", track.source, track.title, track.artist));
        } else {
            msg.push_str(&format!("{}. {} **{}** - {}\n", i, track.source, track.title, track.artist));
        }
    }

    ctx.say(&msg).await?;
    Ok(())
}
