use poise::serenity_prelude::{Colour, CreateEmbed, CreateEmbedFooter};

use crate::commands::play::{linked_title, source_info};
use crate::domain::track::TrackSource;
use crate::services::error::MusicError;
use crate::services::queue_service::QueueService;
use crate::{Context, Error};

const QUEUE_COLOR: Colour = Colour::new(0x5865F2);

/// Show the current music queue
#[poise::command(slash_command, guild_only)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;
    let data = ctx.data();

    let current = QueueService::current(&data.guild_queues, guild_id).await;
    let upcoming = QueueService::list(&data.guild_queues, guild_id).await;

    let Some(current) = current else {
        return Err(MusicError::EmptyQueue.into());
    };

    let (_, color, _) = source_info(&current.source);
    let duration = current.duration.as_deref().unwrap_or("--:--");

    // Now playing embed
    let mut now_playing = CreateEmbed::new()
        .title("Now playing")
        .description(format!("{} - `{}`", linked_title(&current), duration))
        .colour(color);

    if let Some(url) = &current.thumbnail_url {
        now_playing = now_playing.thumbnail(url);
    }

    let mut reply = poise::CreateReply::default().embed(now_playing);

    // Up next embed (if there are queued tracks)
    if !upcoming.is_empty() {
        const MAX_DISPLAY: usize = 10;
        let mut desc = String::new();

        for (i, track) in upcoming.iter().take(MAX_DISPLAY).enumerate() {
            let d = track.duration.as_deref().unwrap_or("--:--");
            let icon = match track.source {
                TrackSource::Spotify => "[SP]",
                TrackSource::YouTube => "[YT]",
            };
            desc.push_str(&format!(
                "`{}.` {} {} - `{}`\n",
                i + 1,
                icon,
                linked_title(track),
                d
            ));
        }

        let remaining = upcoming.len().saturating_sub(MAX_DISPLAY);
        let footer_text = if remaining > 0 {
            format!("{} tracks in queue (+{} more)", upcoming.len(), remaining)
        } else {
            format!("{} tracks in queue", upcoming.len())
        };

        let queue_embed = CreateEmbed::new()
            .title("Up next")
            .description(desc)
            .colour(QUEUE_COLOR)
            .footer(CreateEmbedFooter::new(footer_text));

        reply = reply.embed(queue_embed);
    }

    ctx.send(reply).await?;
    Ok(())
}
