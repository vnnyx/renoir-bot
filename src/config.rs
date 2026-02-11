use std::env;

pub struct Config {
    pub discord_token: String,
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
    pub youtube_api_key: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            discord_token: env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN"),
            spotify_client_id: env::var("SPOTIFY_CLIENT_ID").expect("Missing SPOTIFY_CLIENT_ID"),
            spotify_client_secret: env::var("SPOTIFY_CLIENT_SECRET")
                .expect("Missing SPOTIFY_CLIENT_SECRET"),
            youtube_api_key: env::var("YOUTUBE_API_KEY").expect("Missing YOUTUBE_API_KEY"),
        }
    }
}
