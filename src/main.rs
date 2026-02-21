mod commands;
mod config;
mod domain;
mod infrastructure;
mod services;

use poise::serenity_prelude as serenity;
use songbird::SerenityInit;

use config::Config;
use infrastructure::spotify::SpotifyClient;
use infrastructure::youtube::YouTubeClient;
use services::music_service::MusicService;
use services::queue_service::{GuildQueues, QueueService};

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify, RwLock};

pub type InactivityHandles = Arc<RwLock<HashMap<serenity::GuildId, Arc<Notify>>>>;
pub type EnqueueLocks = Arc<RwLock<HashMap<serenity::GuildId, Arc<Mutex<()>>>>>;
pub type EnqueueCancels = Arc<RwLock<HashMap<serenity::GuildId, Vec<Arc<AtomicBool>>>>>;
pub type JoinLocks = Arc<RwLock<HashMap<serenity::GuildId, Arc<Mutex<()>>>>>;
pub type NowPlayingMessages =
    Arc<RwLock<HashMap<serenity::GuildId, (serenity::ChannelId, serenity::MessageId)>>>;
pub type RepeatStates = Arc<RwLock<HashMap<serenity::GuildId, bool>>>;

pub struct Data {
    pub music_service: MusicService,
    pub guild_queues: GuildQueues,
    pub http_client: reqwest::Client,
    pub inactivity_handles: InactivityHandles,
    pub enqueue_locks: EnqueueLocks,
    pub enqueue_cancels: EnqueueCancels,
    pub join_locks: JoinLocks,
    pub now_playing_messages: NowPlayingMessages,
    pub repeat_states: RepeatStates,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let config = Config::from_env();

    let http_client = reqwest::Client::new();

    let spotify = SpotifyClient::new(&config.spotify_client_id, &config.spotify_client_secret).await;
    let youtube = YouTubeClient::new(http_client.clone(), config.youtube_api_key);
    let music_service = MusicService::new(spotify, youtube);

    let guild_queues = QueueService::new_guild_queues();

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::play::play(),
                commands::stop::stop(),
                commands::next::next(),
                commands::skip::skip(),
                commands::list::list(),
            ],
            event_handler: |ctx, event, _framework, data| {
                Box::pin(async move {
                    if let serenity::FullEvent::InteractionCreate { interaction } = event {
                        if let Some(component) = interaction.as_message_component() {
                            if component.data.custom_id.starts_with("np_") {
                                commands::now_playing::handle_now_playing_interaction(
                                    ctx, component, data,
                                )
                                .await;
                            }
                        }
                    }
                    Ok(())
                })
            },
            on_error: |error| {
                Box::pin(async move {
                    match error {
                        poise::FrameworkError::Command { error, ctx, .. } => {
                            let msg = error.to_string();
                            tracing::warn!("Command error: {msg}");
                            let _ = ctx.say(format!("❌ {msg}")).await;
                        }
                        other => {
                            if let Err(e) = poise::builtins::on_error(other).await {
                                tracing::error!("Error handling error: {e}");
                            }
                        }
                    }
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("Bot is ready!");
                let inactivity_handles = Arc::new(RwLock::new(HashMap::new()));
                let enqueue_locks = Arc::new(RwLock::new(HashMap::new()));
                let enqueue_cancels = Arc::new(RwLock::new(HashMap::new()));
                let join_locks = Arc::new(RwLock::new(HashMap::new()));
                let now_playing_messages = Arc::new(RwLock::new(HashMap::new()));
                let repeat_states = Arc::new(RwLock::new(HashMap::new()));
                Ok(Data {
                    music_service,
                    guild_queues,
                    http_client,
                    inactivity_handles,
                    enqueue_locks,
                    enqueue_cancels,
                    join_locks,
                    now_playing_messages,
                    repeat_states,
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(&config.discord_token, intents)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Failed to create client");

    client.start().await.expect("Client error");
}
