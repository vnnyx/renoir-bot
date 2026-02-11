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

pub struct Data {
    pub music_service: MusicService,
    pub guild_queues: GuildQueues,
    pub http_client: reqwest::Client,
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
                commands::list::list(),
            ],
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("Bot is ready!");
                Ok(Data {
                    music_service,
                    guild_queues,
                    http_client,
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
