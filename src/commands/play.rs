use poise::serenity_prelude::{Colour, CreateEmbed, CreateEmbedAuthor};

use crate::domain::track::{Track, TrackSource};
use crate::infrastructure::audio::AudioSource;
use crate::services::error::MusicError;
use crate::services::music_service::{MusicService, SpotifyUrl};
use crate::services::queue_service::QueueService;
use crate::{Context, Error};

const SPOTIFY_ICON: &str = "https://upload.wikimedia.org/wikipedia/commons/thumb/1/19/Spotify_logo_without_text.svg/168px-Spotify_logo_without_text.svg.png";
const YOUTUBE_ICON: &str = "https://upload.wikimedia.org/wikipedia/commons/0/09/YouTube_full-color_icon_%282017%29.svg";

const SPOTIFY_COLOR: Colour = Colour::new(0x1DB954);
const YOUTUBE_COLOR: Colour = Colour::new(0xFF0000);

fn source_info(source: &TrackSource) -> (&'static str, Colour, &'static str) {
    match source {
        TrackSource::Spotify => (SPOTIFY_ICON, SPOTIFY_COLOR, "Spotify"),
        TrackSource::YouTube => (YOUTUBE_ICON, YOUTUBE_COLOR, "YouTube"),
    }
}

fn enqueue_embed(track: &Track) -> CreateEmbed {
    let (icon, color, source_name) = source_info(&track.source);
    let duration = track.duration.as_deref().unwrap_or("--:--");

    CreateEmbed::new()
        .author(CreateEmbedAuthor::new(source_name).icon_url(icon))
        .description(format!(
            "Added **{}** - {} - `{}`  to the queue.",
            track.title, track.artist, duration
        ))
        .colour(color)
}

fn now_playing_embed(track: &Track, requester: &str) -> CreateEmbed {
    let (_, color, _) = source_info(&track.source);
    let duration = track.duration.as_deref().unwrap_or("--:--");

    let mut embed = CreateEmbed::new()
        .title("Now playing")
        .description(format!(
            "**{}** - {} - `{}`\n\nRequested by {}",
            track.title, track.artist, duration, requester
        ))
        .colour(color);

    if let Some(url) = &track.thumbnail_url {
        embed = embed.thumbnail(url);
    }

    embed
}

fn playlist_embed(count: usize) -> CreateEmbed {
    CreateEmbed::new()
        .author(CreateEmbedAuthor::new("Spotify").icon_url(SPOTIFY_ICON))
        .description(format!("Added **{count}** tracks from playlist to the queue."))
        .colour(SPOTIFY_COLOR)
}

/// Play a song from YouTube or Spotify
#[poise::command(slash_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "YouTube/Spotify URL or search query"] query: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    let channel_id = {
        let guild = ctx.guild().ok_or(MusicError::NotInGuild)?;
        guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|vs| vs.channel_id)
            .ok_or(MusicError::NotInVoiceChannel)?
    };

    ctx.defer().await?;

    let data = ctx.data();
    let http = &data.http_client;
    let requester = format!("<@{}>", ctx.author().id);

    if MusicService::is_youtube_url(&query) {
        let track = Track {
            title: query.clone(),
            artist: String::from("YouTube"),
            url: query.clone(),
            source: TrackSource::YouTube,
            duration: None,
            thumbnail_url: None,
        };
        let input = AudioSource::from_url(http.clone(), &query);

        let manager = songbird::get(ctx.serenity_context())
            .await
            .expect("Songbird not registered");

        let handler_lock = manager
            .join(guild_id, channel_id)
            .await
            .map_err(|e| MusicError::JoinError(e.to_string()))?;

        let is_empty = {
            let handler = handler_lock.lock().await;
            handler.queue().is_empty()
        };

        {
            let mut handler = handler_lock.lock().await;
            handler.enqueue_input(input).await;
        }

        QueueService::add_track(&data.guild_queues, guild_id, track.clone()).await;

        if is_empty {
            ctx.send(poise::CreateReply::default().embed(now_playing_embed(&track, &requester)))
                .await?;
        } else {
            ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
                .await?;
        }
    } else if let Some(spotify_url) = MusicService::parse_spotify_url(&query) {
        match spotify_url {
            SpotifyUrl::Track(id) => {
                let track = data
                    .music_service
                    .spotify
                    .get_track(&id)
                    .await
                    .ok_or(MusicError::NoResults)?;

                let search_query = MusicService::spotify_to_youtube_query(&track);
                let input = AudioSource::from_search(http.clone(), &search_query);

                let manager = songbird::get(ctx.serenity_context())
                    .await
                    .expect("Songbird not registered");

                let handler_lock = manager
                    .join(guild_id, channel_id)
                    .await
                    .map_err(|e| MusicError::JoinError(e.to_string()))?;

                let is_empty = {
                    let handler = handler_lock.lock().await;
                    handler.queue().is_empty()
                };

                {
                    let mut handler = handler_lock.lock().await;
                    handler.enqueue_input(input).await;
                }

                QueueService::add_track(&data.guild_queues, guild_id, track.clone()).await;

                if is_empty {
                    ctx.send(
                        poise::CreateReply::default()
                            .embed(now_playing_embed(&track, &requester)),
                    )
                    .await?;
                } else {
                    ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
                        .await?;
                }
            }
            SpotifyUrl::Playlist(id) => {
                let tracks = data.music_service.spotify.get_playlist_tracks(&id).await;
                if tracks.is_empty() {
                    return Err(MusicError::NoResults.into());
                }

                let manager = songbird::get(ctx.serenity_context())
                    .await
                    .expect("Songbird not registered");

                let handler_lock = manager
                    .join(guild_id, channel_id)
                    .await
                    .map_err(|e| MusicError::JoinError(e.to_string()))?;

                let is_empty = {
                    let handler = handler_lock.lock().await;
                    handler.queue().is_empty()
                };

                let first_track = tracks.first().cloned();
                let count = tracks.len();

                for track in tracks {
                    let search_query = MusicService::spotify_to_youtube_query(&track);
                    let input = AudioSource::from_search(http.clone(), &search_query);

                    {
                        let mut handler = handler_lock.lock().await;
                        handler.enqueue_input(input).await;
                    }

                    QueueService::add_track(&data.guild_queues, guild_id, track).await;
                }

                let mut reply = poise::CreateReply::default().embed(playlist_embed(count));
                if is_empty {
                    if let Some(track) = &first_track {
                        reply = reply.embed(now_playing_embed(track, &requester));
                    }
                }
                ctx.send(reply).await?;
            }
        }
    } else {
        let results = data.music_service.search(&query, 5).await;
        if results.is_empty() {
            return Err(MusicError::NoResults.into());
        }

        let track = results.into_iter().next().unwrap();
        let input = match track.source {
            TrackSource::YouTube => AudioSource::from_url(http.clone(), &track.url),
            TrackSource::Spotify => {
                let search_query = MusicService::spotify_to_youtube_query(&track);
                AudioSource::from_search(http.clone(), &search_query)
            }
        };

        let manager = songbird::get(ctx.serenity_context())
            .await
            .expect("Songbird not registered");

        let handler_lock = manager
            .join(guild_id, channel_id)
            .await
            .map_err(|e| MusicError::JoinError(e.to_string()))?;

        let is_empty = {
            let handler = handler_lock.lock().await;
            handler.queue().is_empty()
        };

        {
            let mut handler = handler_lock.lock().await;
            handler.enqueue_input(input).await;
        }

        QueueService::add_track(&data.guild_queues, guild_id, track.clone()).await;

        if is_empty {
            ctx.send(poise::CreateReply::default().embed(now_playing_embed(&track, &requester)))
                .await?;
        } else {
            ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
                .await?;
        }
    }

    Ok(())
}
