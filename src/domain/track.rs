use std::fmt;

#[derive(Debug, Clone)]
pub enum TrackSource {
    YouTube,
    Spotify,
}

impl fmt::Display for TrackSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackSource::YouTube => write!(f, "[YT]"),
            TrackSource::Spotify => write!(f, "[SP]"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub url: String,
    pub source: TrackSource,
    pub duration: Option<String>,
    pub thumbnail_url: Option<String>,
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} - {}", self.source, self.title, self.artist)
    }
}
