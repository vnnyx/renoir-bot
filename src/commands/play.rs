use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use poise::serenity_prelude::{
    AutocompleteChoice, ChannelId, Colour, CreateEmbed, CreateEmbedAuthor, CreateMessage, GuildId,
    Http,
};
use songbird::events::{Event, EventContext, EventHandler, TrackEvent};
use songbird::Call;
use tokio::sync::Mutex;

use crate::domain::track::{Track, TrackSource};
use crate::infrastructure::audio::AudioSource;
use crate::infrastructure::inactivity::spawn_inactivity_monitor;
use crate::services::cleanup::cleanup_guild;
use crate::services::error::MusicError;
use crate::services::music_service::{MusicService, SpotifyUrl};
use crate::services::queue_service::{GuildQueues, QueueService};
use crate::{Context, EnqueueCancels, Error, InactivityHandles, JoinLocks, NowPlayingMessages};

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
    guild_id: GuildId,
    track: Track,
    requester: String,
    now_playing_messages: NowPlayingMessages,
}

#[async_trait]
impl EventHandler for NowPlayingNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        // Delete the previous "Now Playing" message
        if let Some((ch, msg_id)) = self
            .now_playing_messages
            .write()
            .await
            .remove(&self.guild_id)
        {
            let _ = ch.delete_message(&self.http, msg_id).await;
        }

        let embed = now_playing_embed(&self.track, &self.requester);
        let components =
            super::now_playing::build_now_playing_components(self.guild_id, false);
        let message = CreateMessage::new().embed(embed).components(components);
        match self.channel_id.send_message(&self.http, message).await {
            Ok(msg) => {
                self.now_playing_messages
                    .write()
                    .await
                    .insert(self.guild_id, (self.channel_id, msg.id));
            }
            Err(e) => {
                tracing::warn!("Failed to send Now Playing message: {e}");
            }
        }
        None
    }
}

struct DisconnectCleanup {
    guild_id: GuildId,
    http: Arc<Http>,
    guild_queues: GuildQueues,
    enqueue_cancels: EnqueueCancels,
    inactivity_handles: InactivityHandles,
    now_playing_messages: NowPlayingMessages,
}

#[async_trait]
impl EventHandler for DisconnectCleanup {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        tracing::info!("Bot disconnected from guild {}, cleaning up", self.guild_id);
        cleanup_guild(
            self.guild_id,
            &self.guild_queues,
            &self.enqueue_cancels,
            &self.inactivity_handles,
            &self.now_playing_messages,
            &self.http,
        )
        .await;
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
    now_playing_messages: &NowPlayingMessages,
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
                guild_id,
                track: track.clone(),
                requester: requester.to_string(),
                now_playing_messages: now_playing_messages.clone(),
            },
        );
    }

    QueueService::add_track(guild_queues, guild_id, track.clone()).await;
}

async fn enqueue_collection_tracks(
    tracks: Vec<Track>,
    http_client: reqwest::Client,
    handler_lock: Arc<Mutex<Call>>,
    serenity_http: Arc<Http>,
    channel_id: ChannelId,
    requester: String,
    guild_queues: GuildQueues,
    guild_id: GuildId,
    enqueue_mutex: Arc<Mutex<()>>,
    cancel_flag: Arc<AtomicBool>,
    now_playing_messages: NowPlayingMessages,
) {
    // Acquire per-guild lock so collections are enqueued sequentially
    let _guard = enqueue_mutex.lock_owned().await;

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
            &now_playing_messages,
        )
        .await;
    }

    tracing::info!(
        "Background enqueue complete: {} tracks for guild {}",
        tracks.len(),
        guild_id
    );
}

async fn ensure_voice_connection(
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
    voice_channel_id: ChannelId,
    join_locks: &JoinLocks,
    inactivity_handles: &InactivityHandles,
) -> Result<Arc<Mutex<Call>>, MusicError> {
    // Fast path: already connected AND has active session
    if inactivity_handles.read().await.contains_key(&guild_id) {
        if let Some(handler) = manager.get(guild_id) {
            return Ok(handler);
        }
    }

    // Remove stale handler if present (e.g. after /stop)
    let _ = manager.leave(guild_id).await;

    // Slow path: acquire per-guild lock to prevent concurrent joins
    let lock = {
        let mut locks = join_locks.write().await;
        locks
            .entry(guild_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().await;

    // Double-check after acquiring lock
    if inactivity_handles.read().await.contains_key(&guild_id) {
        if let Some(handler) = manager.get(guild_id) {
            return Ok(handler);
        }
    }

    manager
        .join(guild_id, voice_channel_id)
        .await
        .map_err(|e| MusicError::JoinError(e.to_string()))
}

async fn autocomplete_query(ctx: Context<'_>, partial: &str) -> Vec<AutocompleteChoice> {
    let partial = partial.trim();

    if partial.len() < 3 || partial.starts_with("http://") || partial.starts_with("https://") {
        return Vec::new();
    }

    let results = ctx.data().music_service.search(partial, 5).await;

    results
        .into_iter()
        .take(25)
        .map(|track| {
            let name = format!("{}", track);
            let name = if name.len() > 100 {
                format!("{}...", &name.chars().take(97).collect::<String>())
            } else {
                name
            };
            AutocompleteChoice::new(name, track.url)
        })
        .collect()
}

/// Play a song from YouTube or Spotify
#[poise::command(slash_command, guild_only)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "YouTube/Spotify URL or search query"]
    #[autocomplete = "autocomplete_query"]
    query: String,
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

    let manager = songbird::get(ctx.serenity_context())
        .await
        .expect("Songbird not registered");

    let join_fut = ensure_voice_connection(&manager, guild_id, voice_channel_id, &data.join_locks, &data.inactivity_handles);

    if MusicService::is_youtube_playlist_url(&query) {
        // YouTube playlist — parallelize join + metadata fetch
        let playlist_id = MusicService::extract_youtube_playlist_id(&query)
            .ok_or(MusicError::NoResults)?;

        let ((tracks, name), join_result) = tokio::join!(
            async {
                tokio::join!(
                    data.music_service.youtube.get_playlist_tracks(&playlist_id),
                    data.music_service.youtube.get_playlist_name(&playlist_id),
                )
            },
            join_fut,
        );
        let handler_lock = join_result?;

        if tracks.is_empty() {
            return Err(MusicError::NoResults.into());
        }

        // Fresh join setup
        setup_fresh_join(
            &data, &handler_lock, &manager, guild_id, voice_channel_id,
            text_channel_id, &serenity_http, ctx,
        ).await;

        let name = name.unwrap_or_else(|| "Playlist".to_string());
        let url = format!("https://www.youtube.com/playlist?list={playlist_id}");
        let count = tracks.len();

        ctx.send(
            poise::CreateReply::default()
                .embed(collection_embed(&name, &url, count, &TrackSource::YouTube)),
        )
        .await?;

        spawn_background_enqueue(
            data, tracks, http, handler_lock, serenity_http,
            text_channel_id, requester, guild_id,
        ).await;
    } else if MusicService::is_youtube_url(&query) {
        // YouTube single URL — parallelize join + video lookup
        let video_id = MusicService::extract_youtube_video_id(&query);
        let resolve_fut = async {
            if let Some(vid) = video_id {
                data.music_service
                    .youtube
                    .get_video(&vid)
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
            }
        };

        let (join_result, track) = tokio::join!(join_fut, resolve_fut);
        let handler_lock = join_result?;

        setup_fresh_join(
            &data, &handler_lock, &manager, guild_id, voice_channel_id,
            text_channel_id, &serenity_http, ctx,
        ).await;

        enqueue_track(
            &track, "", http, &handler_lock, &serenity_http,
            text_channel_id, &requester, &data.guild_queues, guild_id,
            &data.now_playing_messages,
        )
        .await;

        ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
            .await?;
    } else if let Some(spotify_url) = MusicService::parse_spotify_url(&query) {
        match spotify_url {
            SpotifyUrl::Track(id) => {
                let (join_result, track_opt) = tokio::join!(
                    join_fut,
                    data.music_service.spotify.get_track(&id),
                );
                let handler_lock = join_result?;
                let track = track_opt.ok_or(MusicError::NoResults)?;

                setup_fresh_join(
                    &data, &handler_lock, &manager, guild_id, voice_channel_id,
                    text_channel_id, &serenity_http, ctx,
                ).await;

                let search_query = MusicService::spotify_to_youtube_query(&track);
                enqueue_track(
                    &track, &search_query, http, &handler_lock, &serenity_http,
                    text_channel_id, &requester, &data.guild_queues, guild_id,
                    &data.now_playing_messages,
                )
                .await;

                ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
                    .await?;
            }
            SpotifyUrl::Playlist(id) => {
                let ((tracks, name), join_result) = tokio::join!(
                    async {
                        tokio::join!(
                            data.music_service.spotify.get_playlist_tracks(&id),
                            data.music_service.spotify.get_playlist_name(&id),
                        )
                    },
                    join_fut,
                );
                let handler_lock = join_result?;

                if tracks.is_empty() {
                    return Err(MusicError::NoResults.into());
                }

                setup_fresh_join(
                    &data, &handler_lock, &manager, guild_id, voice_channel_id,
                    text_channel_id, &serenity_http, ctx,
                ).await;

                let name = name.unwrap_or_else(|| "Playlist".to_string());
                let url = format!("https://open.spotify.com/playlist/{id}");
                let count = tracks.len();

                ctx.send(
                    poise::CreateReply::default()
                        .embed(collection_embed(&name, &url, count, &TrackSource::Spotify)),
                )
                .await?;

                spawn_background_enqueue(
                    data, tracks, http, handler_lock, serenity_http,
                    text_channel_id, requester, guild_id,
                ).await;
            }
            SpotifyUrl::Album(id) => {
                let ((tracks, name), join_result) = tokio::join!(
                    async {
                        tokio::join!(
                            data.music_service.spotify.get_album_tracks(&id),
                            data.music_service.spotify.get_album_name(&id),
                        )
                    },
                    join_fut,
                );
                let handler_lock = join_result?;

                if tracks.is_empty() {
                    return Err(MusicError::NoResults.into());
                }

                setup_fresh_join(
                    &data, &handler_lock, &manager, guild_id, voice_channel_id,
                    text_channel_id, &serenity_http, ctx,
                ).await;

                let name = name.unwrap_or_else(|| "Album".to_string());
                let url = format!("https://open.spotify.com/album/{id}");
                let count = tracks.len();

                ctx.send(
                    poise::CreateReply::default()
                        .embed(collection_embed(&name, &url, count, &TrackSource::Spotify)),
                )
                .await?;

                spawn_background_enqueue(
                    data, tracks, http, handler_lock, serenity_http,
                    text_channel_id, requester, guild_id,
                ).await;
            }
        }
    } else {
        // Search query — parallelize join + search
        let (join_result, results) = tokio::join!(
            join_fut,
            data.music_service.search(&query, 5),
        );
        let handler_lock = join_result?;

        if results.is_empty() {
            return Err(MusicError::NoResults.into());
        }

        setup_fresh_join(
            &data, &handler_lock, &manager, guild_id, voice_channel_id,
            text_channel_id, &serenity_http, ctx,
        ).await;

        let track = results.into_iter().next().unwrap();
        let search_query = match track.source {
            TrackSource::YouTube => String::new(),
            TrackSource::Spotify => MusicService::spotify_to_youtube_query(&track),
        };

        enqueue_track(
            &track, &search_query, http, &handler_lock, &serenity_http,
            text_channel_id, &requester, &data.guild_queues, guild_id,
            &data.now_playing_messages,
        )
        .await;

        ctx.send(poise::CreateReply::default().embed(enqueue_embed(&track)))
            .await?;
    }

    Ok(())
}

async fn setup_fresh_join(
    data: &crate::Data,
    handler_lock: &Arc<Mutex<Call>>,
    manager: &Arc<songbird::Songbird>,
    guild_id: GuildId,
    voice_channel_id: ChannelId,
    text_channel_id: ChannelId,
    serenity_http: &Arc<Http>,
    ctx: Context<'_>,
) {
    let mut handles = data.inactivity_handles.write().await;
    if !handles.contains_key(&guild_id) {
        {
            let handler = handler_lock.lock().await;
            handler.queue().stop();
        }
        {
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(
                Event::Core(songbird::CoreEvent::DriverDisconnect),
                DisconnectCleanup {
                    guild_id,
                    http: serenity_http.clone(),
                    guild_queues: data.guild_queues.clone(),
                    enqueue_cancels: data.enqueue_cancels.clone(),
                    inactivity_handles: data.inactivity_handles.clone(),
                    now_playing_messages: data.now_playing_messages.clone(),
                },
            );
        }
        handles.insert(
            guild_id,
            spawn_inactivity_monitor(
                manager.clone(),
                guild_id,
                voice_channel_id,
                text_channel_id,
                serenity_http.clone(),
                ctx.serenity_context().cache.clone(),
                data.guild_queues.clone(),
                data.inactivity_handles.clone(),
                data.enqueue_cancels.clone(),
                data.now_playing_messages.clone(),
            ),
        );
    }
}

async fn spawn_background_enqueue(
    data: &crate::Data,
    tracks: Vec<Track>,
    http: &reqwest::Client,
    handler_lock: Arc<Mutex<Call>>,
    serenity_http: Arc<Http>,
    text_channel_id: ChannelId,
    requester: String,
    guild_id: GuildId,
) {
    let enqueue_mutex = {
        let mut locks = data.enqueue_locks.write().await;
        locks.entry(guild_id).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
    };
    let cancel_flag = Arc::new(AtomicBool::new(false));
    data.enqueue_cancels.write().await.entry(guild_id).or_default().push(cancel_flag.clone());

    tokio::spawn(enqueue_collection_tracks(
        tracks,
        http.clone(),
        handler_lock,
        serenity_http,
        text_channel_id,
        requester,
        data.guild_queues.clone(),
        guild_id,
        enqueue_mutex,
        cancel_flag,
        data.now_playing_messages.clone(),
    ));
}
