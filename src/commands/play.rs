use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use poise::serenity_prelude::{
    ChannelId, Colour, CreateEmbed, CreateEmbedAuthor, CreateMessage, GuildId, Http,
};
use songbird::events::{Event, EventContext, EventHandler, TrackEvent};
use songbird::Call;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::domain::track::{Track, TrackSource};
use crate::infrastructure::audio::AudioSource;
use crate::infrastructure::inactivity::spawn_inactivity_monitor;
use crate::services::error::MusicError;
use crate::services::music_service::{MusicService, SpotifyUrl};
use crate::services::queue_service::{GuildQueues, QueueService};
use crate::{Context, Error};

pub const SPOTIFY_ICON: &str = "https://upload.wikimedia.org/wikipedia/commons/thumb/1/19/Spotify_logo_without_text.svg/168px-Spotify_logo_without_text.svg.png";
pub const YOUTUBE_ICON: &str = "https://www.gstatic.com/images/branding/product/2x/youtube_64dp.png";

const SPOTIFY_COLOR: Colour = Colour::new(0x1DB954);
const YOUTUBE_COLOR: Colour = Colour::new(0xFF0000);

pub fn source_info(source: &TrackSource) -> (&'static str, Colour, &'static str) {
    match source {
        TrackSource::Spotify => (SPOTIFY_ICON, SPOTIFY_COLOR, "Spotify"),
        TrackSource::YouTube => (YOUTUBE_ICON, YOUTUBE_COLOR, "YouTube"),
    }
}

pub fn linked_title(track: &Track) -> String {
    if track.url.is_empty() {
        format!("**{}** - {}", track.title, track.artist)
    } else {
        format!("[**{}** - {}]({})", track.title, track.artist, track.url)
    }
}

fn enqueue_embed(track: &Track) -> CreateEmbed {
    let (icon, color, source_name) = source_info(&track.source);
    let duration = track.duration.as_deref().unwrap_or("--:--");

    CreateEmbed::new()
        .author(CreateEmbedAuthor::new(source_name).icon_url(icon))
        .description(format!(
            "Added {} - `{}`  to the queue.",
            linked_title(track), duration
        ))
        .colour(color)
}

pub fn now_playing_embed(track: &Track, requester: &str) -> CreateEmbed {
    let (_, color, _) = source_info(&track.source);
    let duration = track.duration.as_deref().unwrap_or("--:--");

    let mut embed = CreateEmbed::new()
        .title("Now playing")
        .description(format!(
            "{} - `{}`\n\nRequested by {}",
            linked_title(track), duration, requester
        ))
        .colour(color);

    if let Some(url) = &track.thumbnail_url {
        embed = embed.thumbnail(url);
    }

    embed
}

fn collection_embed(name: &str, url: &str, count: usize, source: &TrackSource) -> CreateEmbed {
    let (icon, color, source_name) = source_info(source);
    let linked_name = if url.is_empty() {
        format!("**{name}**")
    } else {
        format!("[**{name}**]({url})")
    };

    CreateEmbed::new()
        .author(CreateEmbedAuthor::new(source_name).icon_url(icon))
        .description(format!(
            "Added {linked_name} with `{count}` tracks to the queue."
        ))
        .colour(color)
}

struct NowPlayingNotifier {
    http: Arc<Http>,
    channel_id: ChannelId,
    track: Track,
    requester: String,
}

#[async_trait]
impl EventHandler for NowPlayingNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let embed = now_playing_embed(&self.track, &self.requester);
        let message = CreateMessage::new().embed(embed);
        if let Err(e) = self.channel_id.send_message(&self.http, message).await {
            tracing::warn!("Failed to send Now Playing message: {e}");
        }
        None
    }
}

async fn enqueue_track(
    track: &Track,
    search_query: &str,
    http_client: &reqwest::Client,
    handler_lock: &Arc<Mutex<Call>>,
    serenity_http: &Arc<Http>,
    channel_id: ChannelId,
    requester: &str,
    guild_queues: &GuildQueues,
    guild_id: GuildId,
) {
    let input = if search_query.is_empty() {
        AudioSource::from_url(http_client.clone(), &track.url)
    } else {
        AudioSource::from_search(http_client.clone(), search_query)
    };

    {
        let mut handler = handler_lock.lock().await;
        let track_handle = handler.enqueue_input(input).await;
        let _ = track_handle.add_event(
            Event::Track(TrackEvent::Play),
            NowPlayingNotifier {
                http: serenity_http.clone(),
                channel_id,
                track: track.clone(),
                requester: requester.to_string(),
            },
        );
    }

    QueueService::add_track(guild_queues, guild_id, track.clone()).await;
}

async fn enqueue_remaining_tracks(
    tracks: Vec<Track>,
    http_client: reqwest::Client,
    handler_lock: Arc<Mutex<Call>>,
    serenity_http: Arc<Http>,
    channel_id: ChannelId,
    requester: String,
    guild_queues: GuildQueues,
    guild_id: GuildId,
    _enqueue_guard: OwnedMutexGuard<()>,
    cancel_flag: Arc<AtomicBool>,
) {
    for track in &tracks {
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("Background enqueue cancelled for guild {guild_id}");
            return;
        }

        let search_query = match track.source {
            TrackSource::Spotify => MusicService::spotify_to_youtube_query(track),
            TrackSource::YouTube => String::new(),
        };

        enqueue_track(
            track,
            &search_query,
            &http_client,
            &handler_lock,
            &serenity_http,
            channel_id,
            &requester,
            &guild_queues,
            guild_id,
        )
        .await;
    }

    tracing::info!(
        "Background enqueue complete: {} tracks for guild {}",
        tracks.len(),
        guild_id
    );
}

/// Play a song from YouTube or Spotify
#[poise::command(slash_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "YouTube/Spotify URL or search query"] query: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or(MusicError::NotInGuild)?;

    let voice_channel_id = {
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
    let serenity_http = ctx.serenity_context().http.clone();
    let text_channel_id = ctx.channel_id();
    let requester = format!("<@{}>", ctx.author().id);

    // Join voice channel
    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird not registered");

    let handler_lock = manager
        .join(guild_id, voice_channel_id)
        .await
        .map_err(|e| MusicError::JoinError(e.to_string()))?;

    // Spawn inactivity monitor if not already running for this guild
    {
        let mut handles = data.inactivity_handles.write().await;
        handles.entry(guild_id).or_insert_with(|| {
            spawn_inactivity_monitor(
                manager.clone(),
                guild_id,
                voice_channel_id,
                text_channel_id,
                serenity_http.clone(),
                ctx.serenity_context().cache.clone(),
                data.guild_queues.clone(),
                data.inactivity_handles.clone(),
            )
        });
    }

    if MusicService::is_youtube_playlist_url(&query) {
        // YouTube playlist
        let playlist_id = MusicService::extract_youtube_playlist_id(&query)
            .ok_or(MusicError::NoResults)?;

        let (tracks, name) = tokio::join!(
            data.music_service.youtube.get_playlist_tracks(&playlist_id),
            data.music_service.youtube.get_playlist_name(&playlist_id),
        );
        if tracks.is_empty() {
            return Err(MusicError::NoResults.into());
        }

        let name = name.unwrap_or_else(|| "Playlist".to_string());
        let url = format!("https://www.youtube.com/playlist?list={playlist_id}");
        let count = tracks.len();

        // Acquire per-guild enqueue lock to serialize collection enqueues
        let enqueue_mutex = {
            let mut locks = data.enqueue_locks.write().await;
            locks.entry(guild_id).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
        };
        let enqueue_guard = enqueue_mutex.lock_owned().await;

        // Enqueue first track immediately
        let first = &tracks[0];
        enqueue_track(
            first,
            "",
            http,
            &handler_lock,
            &serenity_http,
            text_channel_id,
            &requester,
            &data.guild_queues,
            guild_id,
        )
        .await;

        // Respond immediately
        ctx.send(
            poise::CreateReply::default()
                .embed(collection_embed(&name, &url, count, &TrackSource::YouTube)),
        )
        .await?;

        // Background enqueue remaining tracks (guard keeps lock held)
        if tracks.len() > 1 {
            let cancel_flag = Arc::new(AtomicBool::new(false));
            data.enqueue_cancels.write().await.insert(guild_id, cancel_flag.clone());

            let remaining = tracks[1..].to_vec();
            tokio::spawn(enqueue_remaining_tracks(
                remaining,
                http.clone(),
                handler_lock,
                serenity_http,
                text_channel_id,
                requester,
                data.guild_queues.clone(),
                guild_id,
                enqueue_guard,
                cancel_flag,
            ));
        }
    } else if MusicService::is_youtube_url(&query) {
        let track = if let Some(video_id) = MusicService::extract_youtube_video_id(&query) {
            data.music_service
                .youtube
                .get_video(&video_id)
                .await
                .unwrap_or(Track {
                    title: query.clone(),
                    artist: String::from("YouTube"),
                    url: query.clone(),
                    source: TrackSource::YouTube,
                    duration: None,
                    thumbnail_url: None,
                })
        } else {
            Track {
                title: query.clone(),
                artist: String::from("YouTube"),
                url: query.clone(),
                source: TrackSource::YouTube,
                duration: None,
                thumbnail_url: None,
            }
        };

        enqueue_track(
            &track,
            "",
            http,
            &handler_lock,
            &serenity_http,
            text_channel_id,
            &requester,
            &data.guild_queues,
            guild_id,
        )
        .await;

        ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
            .await?;
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
                enqueue_track(
                    &track,
                    &search_query,
                    http,
                    &handler_lock,
                    &serenity_http,
                    text_channel_id,
                    &requester,
                    &data.guild_queues,
                    guild_id,
                )
                .await;

                ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
                    .await?;
            }
            SpotifyUrl::Playlist(id) => {
                let (tracks, name) = tokio::join!(
                    data.music_service.spotify.get_playlist_tracks(&id),
                    data.music_service.spotify.get_playlist_name(&id),
                );
                if tracks.is_empty() {
                    return Err(MusicError::NoResults.into());
                }

                let name = name.unwrap_or_else(|| "Playlist".to_string());
                let url = format!("https://open.spotify.com/playlist/{id}");
                let count = tracks.len();

                // Acquire per-guild enqueue lock to serialize collection enqueues
                let enqueue_mutex = {
                    let mut locks = data.enqueue_locks.write().await;
                    locks.entry(guild_id).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
                };
                let enqueue_guard = enqueue_mutex.lock_owned().await;

                // Enqueue first track immediately
                let first = &tracks[0];
                let search_query = MusicService::spotify_to_youtube_query(first);
                enqueue_track(
                    first,
                    &search_query,
                    http,
                    &handler_lock,
                    &serenity_http,
                    text_channel_id,
                    &requester,
                    &data.guild_queues,
                    guild_id,
                )
                .await;

                // Respond immediately
                ctx.send(
                    poise::CreateReply::default()
                        .embed(collection_embed(&name, &url, count, &TrackSource::Spotify)),
                )
                .await?;

                // Background enqueue remaining tracks (guard keeps lock held)
                if tracks.len() > 1 {
                    let cancel_flag = Arc::new(AtomicBool::new(false));
                    data.enqueue_cancels.write().await.insert(guild_id, cancel_flag.clone());

                    let remaining = tracks[1..].to_vec();
                    tokio::spawn(enqueue_remaining_tracks(
                        remaining,
                        http.clone(),
                        handler_lock,
                        serenity_http,
                        text_channel_id,
                        requester,
                        data.guild_queues.clone(),
                        guild_id,
                        enqueue_guard,
                        cancel_flag,
                    ));
                }
            }
            SpotifyUrl::Album(id) => {
                let (tracks, name) = tokio::join!(
                    data.music_service.spotify.get_album_tracks(&id),
                    data.music_service.spotify.get_album_name(&id),
                );
                if tracks.is_empty() {
                    return Err(MusicError::NoResults.into());
                }

                let name = name.unwrap_or_else(|| "Album".to_string());
                let url = format!("https://open.spotify.com/album/{id}");
                let count = tracks.len();

                // Acquire per-guild enqueue lock to serialize collection enqueues
                let enqueue_mutex = {
                    let mut locks = data.enqueue_locks.write().await;
                    locks.entry(guild_id).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
                };
                let enqueue_guard = enqueue_mutex.lock_owned().await;

                // Enqueue first track immediately
                let first = &tracks[0];
                let search_query = MusicService::spotify_to_youtube_query(first);
                enqueue_track(
                    first,
                    &search_query,
                    http,
                    &handler_lock,
                    &serenity_http,
                    text_channel_id,
                    &requester,
                    &data.guild_queues,
                    guild_id,
                )
                .await;

                // Respond immediately
                ctx.send(
                    poise::CreateReply::default()
                        .embed(collection_embed(&name, &url, count, &TrackSource::Spotify)),
                )
                .await?;

                // Background enqueue remaining tracks (guard keeps lock held)
                if tracks.len() > 1 {
                    let cancel_flag = Arc::new(AtomicBool::new(false));
                    data.enqueue_cancels.write().await.insert(guild_id, cancel_flag.clone());

                    let remaining = tracks[1..].to_vec();
                    tokio::spawn(enqueue_remaining_tracks(
                        remaining,
                        http.clone(),
                        handler_lock,
                        serenity_http,
                        text_channel_id,
                        requester,
                        data.guild_queues.clone(),
                        guild_id,
                        enqueue_guard,
                        cancel_flag,
                    ));
                }
            }
        }
    } else {
        let results = data.music_service.search(&query, 5).await;
        if results.is_empty() {
            return Err(MusicError::NoResults.into());
        }

        let track = results.into_iter().next().unwrap();
        let search_query = match track.source {
            TrackSource::YouTube => String::new(),
            TrackSource::Spotify => MusicService::spotify_to_youtube_query(&track),
        };

        enqueue_track(
            &track,
            &search_query,
            http,
            &handler_lock,
            &serenity_http,
            text_channel_id,
            &requester,
            &data.guild_queues,
            guild_id,
        )
        .await;

        ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
            .await?;
    }

    Ok(())
}
