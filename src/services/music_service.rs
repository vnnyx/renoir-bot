use regex::Regex;
use std::sync::LazyLock;

use crate::domain::track::Track;
use crate::infrastructure::spotify::SpotifyClient;
use crate::infrastructure::youtube::YouTubeClient;

static YOUTUBE_PLAYLIST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"youtube\.com/(?:playlist\?|watch\?.*list=)").unwrap()
});

static YOUTUBE_PLAYLIST_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[?&]list=([a-zA-Z0-9_-]+)").unwrap()
});

static YOUTUBE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:youtube\.com/watch|youtu\.be/|youtube\.com/shorts/)").unwrap()
});

static YOUTUBE_VIDEO_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:youtube\.com/watch\?.*v=|youtu\.be/|youtube\.com/shorts/)([a-zA-Z0-9_-]{11})").unwrap()
});

static SPOTIFY_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"open\.spotify\.com/(track|playlist|album)/([a-zA-Z0-9]+)").unwrap()
});

pub enum SpotifyUrl {
    Track(String),
    Playlist(String),
    Album(String),
}

pub struct MusicService {
    pub spotify: SpotifyClient,
    pub youtube: YouTubeClient,
}

impl MusicService {
    pub fn new(spotify: SpotifyClient, youtube: YouTubeClient) -> Self {
        Self { spotify, youtube }
    }

    pub fn is_youtube_playlist_url(query: &str) -> bool {
        if !YOUTUBE_PLAYLIST_RE.is_match(query) {
            return false;
        }
        // YouTube Radio/Mix playlists (RD prefix) are auto-generated and
        // not accessible via the YouTube Data API. If the URL also has a
        // video ID, treat it as a single video instead.
        if let Some(id) = Self::extract_youtube_playlist_id(query) {
            if id.starts_with("RD") && Self::extract_youtube_video_id(query).is_some() {
                return false;
            }
        }
        true
    }

    pub fn extract_youtube_playlist_id(query: &str) -> Option<String> {
        let caps = YOUTUBE_PLAYLIST_ID_RE.captures(query)?;
        Some(caps.get(1)?.as_str().to_string())
    }

    pub fn is_youtube_url(query: &str) -> bool {
        YOUTUBE_URL_RE.is_match(query)
    }

    pub fn extract_youtube_video_id(query: &str) -> Option<String> {
        let caps = YOUTUBE_VIDEO_ID_RE.captures(query)?;
        Some(caps.get(1)?.as_str().to_string())
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
            "album" => Some(SpotifyUrl::Album(id)),
            _ => None,
        }
    }

    pub async fn search(&self, query: &str, limit: u32) -> Vec<Track> {
        let yt_fut = self.youtube.search_tracks(query, limit);
        let sp_fut = self.spotify.search_tracks(query, limit);
        tokio::pin!(yt_fut);
        tokio::pin!(sp_fut);

        tokio::select! {
            yt = &mut yt_fut => {
                if !yt.is_empty() { return yt; }
                sp_fut.await
            }
            sp = &mut sp_fut => {
                if !sp.is_empty() { return sp; }
                yt_fut.await
            }
        }
    }

    pub fn spotify_to_youtube_query(track: &Track) -> String {
        format!("{} {} audio", track.title, track.artist)
    }
}
