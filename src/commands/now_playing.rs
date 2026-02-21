use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude::{
    self as serenity, ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, GuildId,
};
use songbird::tracks::PlayMode;

use crate::services::cleanup::cleanup_guild;
use crate::services::queue_service::QueueService;
use crate::Data;

pub fn build_now_playing_components(guild_id: GuildId, paused: bool) -> Vec<CreateActionRow> {
    let pause_label = if paused { "▶ Resume" } else { "⏸ Pause" };
    let pause_id = format!("np_pause_{guild_id}");

    let row = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("np_seekback_{guild_id}"))
            .label("⏪ -15s")
            .style(ButtonStyle::Secondary),
        CreateButton::new(pause_id)
            .label(pause_label)
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("np_skip_{guild_id}"))
            .label("⏭ Skip")
            .style(ButtonStyle::Secondary),
        CreateButton::new(format!("np_stop_{guild_id}"))
            .label("⏹ Stop")
            .style(ButtonStyle::Danger),
        CreateButton::new(format!("np_seekfwd_{guild_id}"))
            .label("⏩ +15s")
            .style(ButtonStyle::Secondary),
    ]);

    vec![row]
}

fn parse_custom_id(custom_id: &str) -> Option<(&str, GuildId)> {
    // Format: np_{action}_{guild_id}
    let rest = custom_id.strip_prefix("np_")?;
    let (action, guild_id_str) = rest.rsplit_once('_')?;
    let guild_id: u64 = guild_id_str.parse().ok()?;
    Some((action, GuildId::new(guild_id)))
}

pub async fn handle_now_playing_interaction(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    data: &Data,
) {
    let Some((action, guild_id)) = parse_custom_id(&component.data.custom_id) else {
        return;
    };

    let manager = songbird::get(ctx).await.expect("Songbird not registered");

    match action {
        "pause" => handle_pause(ctx, component, &manager, guild_id).await,
        "skip" => handle_skip(ctx, component, &manager, guild_id, data).await,
        "stop" => handle_stop(ctx, component, &manager, guild_id, data).await,
        "seekback" => handle_seek(ctx, component, &manager, guild_id, false).await,
        "seekfwd" => handle_seek(ctx, component, &manager, guild_id, true).await,
        _ => {}
    }
}

async fn handle_pause(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
) {
    let Some(handler_lock) = manager.get(guild_id) else {
        send_ephemeral(ctx, component, "Not currently playing.").await;
        return;
    };

    let handler = handler_lock.lock().await;
    let Some(current) = handler.queue().current() else {
        send_ephemeral(ctx, component, "No track is currently playing.").await;
        return;
    };

    let info = match current.get_info().await {
        Ok(info) => info,
        Err(_) => {
            send_ephemeral(ctx, component, "Could not get track info.").await;
            return;
        }
    };

    let now_paused = match info.playing {
        PlayMode::Play => {
            let _ = current.pause();
            true
        }
        _ => {
            let _ = current.play();
            false
        }
    };

    // Update the message with toggled button
    let components = build_now_playing_components(guild_id, now_paused);

    let response = CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new().components(components),
    );

    if let Err(e) = component.create_response(&ctx.http, response).await {
        tracing::warn!("Failed to respond to pause interaction: {e}");
    }
}

async fn handle_skip(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
    data: &Data,
) {
    let Some(handler_lock) = manager.get(guild_id) else {
        send_ephemeral(ctx, component, "Not currently playing.").await;
        return;
    };

    {
        let handler = handler_lock.lock().await;
        let queue = handler.queue();
        if queue.is_empty() {
            send_ephemeral(ctx, component, "Queue is empty.").await;
            return;
        }
        let _ = queue.skip();
    }

    let skipped = QueueService::skip(&data.guild_queues, guild_id).await;
    let msg = match skipped {
        Some(track) => format!("Skipped: **{track}**"),
        None => "Skipped current track.".to_string(),
    };

    send_ephemeral(ctx, component, &msg).await;
}

async fn handle_stop(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
    data: &Data,
) {
    cleanup_guild(
        guild_id,
        &data.guild_queues,
        &data.enqueue_cancels,
        &data.inactivity_handles,
        &data.now_playing_messages,
        &ctx.http,
    )
    .await;

    if let Some(handler_lock) = manager.get(guild_id) {
        let handler = handler_lock.lock().await;
        handler.queue().stop();
    }

    let _ = manager.leave(guild_id).await;

    send_ephemeral(ctx, component, "Stopped playback and left the voice channel.").await;
}

async fn handle_seek(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
    forward: bool,
) {
    let Some(handler_lock) = manager.get(guild_id) else {
        send_ephemeral(ctx, component, "Not currently playing.").await;
        return;
    };

    let handler = handler_lock.lock().await;
    let Some(current) = handler.queue().current() else {
        send_ephemeral(ctx, component, "No track is currently playing.").await;
        return;
    };

    let info = match current.get_info().await {
        Ok(info) => info,
        Err(_) => {
            send_ephemeral(ctx, component, "Could not get track info.").await;
            return;
        }
    };

    let position = info.position;
    let new_position = if forward {
        position + Duration::from_secs(15)
    } else {
        position.saturating_sub(Duration::from_secs(15))
    };

    let _ = current.seek(new_position);

    let direction = if forward { "forward" } else { "backward" };
    let secs = new_position.as_secs();
    let msg = format!(
        "Seeked {direction} 15s → `{}:{:02}`",
        secs / 60,
        secs % 60
    );
    send_ephemeral(ctx, component, &msg).await;
}

async fn send_ephemeral(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    content: &str,
) {
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(content)
            .ephemeral(true),
    );

    if let Err(e) = component.create_response(&ctx.http, response).await {
        tracing::warn!("Failed to respond to component interaction: {e}");
    }
}
