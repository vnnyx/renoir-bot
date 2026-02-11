use regex::Regex;
use std::sync::LazyLock;

use crate::domain::track::Track;
use crate::infrastructure::spotify::SpotifyClient;
use crate::infrastructure::youtube::YouTubeClient;

static YOUTUBE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:youtube\.com/watch|youtu\.be/|youtube\.com/shorts/)").unwrap()
});

static SPOTIFY_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"open\.spotify\.com/(track|playlist)/([a-zA-Z0-9]+)").unwrap()
});

pub enum SpotifyUrl {
    Track(String),
    Playlist(String),
}

pub struct MusicService {
    pub spotify: SpotifyClient,
    pub youtube: YouTubeClient,
}

impl MusicService {
    pub fn new(spotify: SpotifyClient, youtube: YouTubeClient) -> Self {
        Self { spotify, youtube }
    }

    pub fn is_youtube_url(query: &str) -> bool {
        YOUTUBE_URL_RE.is_match(query)
    }

    pub fn is_spotify_url(query: &str) -> bool {
        SPOTIFY_URL_RE.is_match(query)
    }

    pub fn parse_spotify_url(query: &str) -> Option<SpotifyUrl> {
        let caps = SPOTIFY_URL_RE.captures(query)?;
        let kind = caps.get(1)?.as_str();
        let id = caps.get(2)?.as_str().to_string();
        match kind {
            "track" => Some(SpotifyUrl::Track(id)),
            "playlist" => Some(SpotifyUrl::Playlist(id)),
            _ => None,
        }
    }

    pub async fn search(&self, query: &str, limit: u32) -> Vec<Track> {
        let (yt_results, sp_results) = tokio::join!(
            self.youtube.search_tracks(query, limit),
            self.spotify.search_tracks(query, limit),
        );

        let mut results = Vec::with_capacity(yt_results.len() + sp_results.len());
        results.extend(yt_results);
        results.extend(sp_results);
        results
    }

    pub fn spotify_to_youtube_query(track: &Track) -> String {
        format!("{} {} audio", track.title, track.artist)
    }
}
