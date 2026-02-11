use reqwest::Client;
use serde::Deserialize;

use crate::domain::track::{Track, TrackSource};

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: VideoId,
    snippet: Snippet,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoId {
    video_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snippet {
    title: String,
    channel_title: String,
    thumbnails: Option<Thumbnails>,
}

#[derive(Deserialize)]
struct Thumbnails {
    high: Option<Thumbnail>,
    default: Option<Thumbnail>,
}

#[derive(Deserialize)]
struct Thumbnail {
    url: String,
}

pub struct YouTubeClient {
    http: Client,
    api_key: String,
}

impl YouTubeClient {
    pub fn new(http: Client, api_key: String) -> Self {
        Self { http, api_key }
    }

    pub async fn search_tracks(&self, query: &str, limit: u32) -> Vec<Track> {
        let resp = self
            .http
            .get("https://www.googleapis.com/youtube/v3/search")
            .query(&[
                ("part", "snippet"),
                ("type", "video"),
                ("q", query),
                ("maxResults", &limit.to_string()),
                ("key", &self.api_key),
            ])
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("YouTube API request failed: {e}");
                return Vec::new();
            }
        };

        let search: SearchResponse = match resp.json().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("YouTube API parse failed: {e}");
                return Vec::new();
            }
        };

        search
            .items
            .into_iter()
            .filter_map(|item| {
                let video_id = item.id.video_id?;
                let thumbnail_url = item
                    .snippet
                    .thumbnails
                    .and_then(|t| t.high.or(t.default))
                    .map(|t| t.url);

                Some(Track {
                    title: item.snippet.title,
                    artist: item.snippet.channel_title,
                    url: format!("https://www.youtube.com/watch?v={video_id}"),
                    source: TrackSource::YouTube,
                    duration: None,
                    thumbnail_url,
                })
            })
            .collect()
    }
}
