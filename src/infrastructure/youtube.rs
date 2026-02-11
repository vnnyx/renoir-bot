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

#[derive(Deserialize)]
struct VideoResponse {
    items: Vec<VideoItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistItemsResponse {
    items: Vec<PlaylistItem>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct PlaylistItem {
    snippet: PlaylistItemSnippet,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistItemSnippet {
    title: String,
    channel_title: String,
    thumbnails: Option<Thumbnails>,
    resource_id: ResourceId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceId {
    video_id: Option<String>,
}

#[derive(Deserialize)]
struct PlaylistResponse {
    items: Vec<PlaylistDetail>,
}

#[derive(Deserialize)]
struct PlaylistDetail {
    snippet: PlaylistDetailSnippet,
}

#[derive(Deserialize)]
struct PlaylistDetailSnippet {
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoItem {
    snippet: Snippet,
    content_details: ContentDetails,
}

#[derive(Deserialize)]
struct ContentDetails {
    duration: String,
}

fn parse_iso8601_duration(duration: &str) -> Option<String> {
    let d = duration.strip_prefix("PT")?;
    let mut minutes = 0u64;
    let mut seconds = 0u64;

    let mut num_buf = String::new();
    for ch in d.chars() {
        match ch {
            'H' => {
                let hours: u64 = num_buf.parse().ok()?;
                minutes += hours * 60;
                num_buf.clear();
            }
            'M' => {
                minutes += num_buf.parse::<u64>().ok()?;
                num_buf.clear();
            }
            'S' => {
                seconds = num_buf.parse().ok()?;
                num_buf.clear();
            }
            _ => num_buf.push(ch),
        }
    }

    Some(format!("{minutes}:{seconds:02}"))
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

    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Vec<Track> {
        let mut tracks = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut params = vec![
                ("part", "snippet".to_string()),
                ("playlistId", playlist_id.to_string()),
                ("maxResults", "50".to_string()),
                ("key", self.api_key.clone()),
            ];
            if let Some(token) = &page_token {
                params.push(("pageToken", token.clone()));
            }

            let resp = self
                .http
                .get("https://www.googleapis.com/youtube/v3/playlistItems")
                .query(&params)
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("YouTube playlistItems API request failed: {e}");
                    break;
                }
            };

            let playlist_resp: PlaylistItemsResponse = match resp.json().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("YouTube playlistItems API parse failed: {e}");
                    break;
                }
            };

            for item in playlist_resp.items {
                if let Some(video_id) = item.snippet.resource_id.video_id {
                    let thumbnail_url = item
                        .snippet
                        .thumbnails
                        .and_then(|t| t.high.or(t.default))
                        .map(|t| t.url);

                    tracks.push(Track {
                        title: item.snippet.title,
                        artist: item.snippet.channel_title,
                        url: format!("https://www.youtube.com/watch?v={video_id}"),
                        source: TrackSource::YouTube,
                        duration: None,
                        thumbnail_url,
                    });
                }
            }

            match playlist_resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        tracks
    }

    pub async fn get_playlist_name(&self, playlist_id: &str) -> Option<String> {
        let resp = self
            .http
            .get("https://www.googleapis.com/youtube/v3/playlists")
            .query(&[
                ("part", "snippet"),
                ("id", playlist_id),
                ("key", &self.api_key),
            ])
            .send()
            .await
            .ok()?;

        let playlist_resp: PlaylistResponse = resp.json().await.ok()?;
        playlist_resp
            .items
            .into_iter()
            .next()
            .map(|item| item.snippet.title)
    }

    pub async fn get_video(&self, video_id: &str) -> Option<Track> {
        let resp = self
            .http
            .get("https://www.googleapis.com/youtube/v3/videos")
            .query(&[
                ("part", "snippet,contentDetails"),
                ("id", video_id),
                ("key", &self.api_key),
            ])
            .send()
            .await
            .ok()?;

        let video_resp: VideoResponse = resp.json().await.ok()?;
        let item = video_resp.items.into_iter().next()?;

        let thumbnail_url = item
            .snippet
            .thumbnails
            .and_then(|t| t.high.or(t.default))
            .map(|t| t.url);

        let duration = parse_iso8601_duration(&item.content_details.duration);

        Some(Track {
            title: item.snippet.title,
            artist: item.snippet.channel_title,
            url: format!("https://www.youtube.com/watch?v={video_id}"),
            source: TrackSource::YouTube,
            duration,
            thumbnail_url,
        })
    }
}
