use futures::stream::TryStreamExt;
use rspotify::model::{AlbumId, PlayableItem, PlaylistId, SearchResult, SimplifiedTrack, TrackId};
use rspotify::{ClientCredsSpotify, Credentials, prelude::*};

use crate::domain::track::{Track, TrackSource};

pub struct SpotifyClient {
    client: ClientCredsSpotify,
}

impl SpotifyClient {
    pub async fn new(client_id: &str, client_secret: &str) -> Self {
        let creds = Credentials::new(client_id, client_secret);
        let client = ClientCredsSpotify::new(creds);
        client.request_token().await.expect("Failed to get Spotify token");
        Self { client }
    }

    pub async fn search_tracks(&self, query: &str, limit: u32) -> Vec<Track> {
        let result = self
            .client
            .search(
                query,
                rspotify::model::SearchType::Track,
                None,
                None,
                Some(limit),
                None,
            )
            .await;

        let result = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Spotify search failed: {e}");
                return Vec::new();
            }
        };

        if let SearchResult::Tracks(page) = result {
            page.items
                .into_iter()
                .map(|track| {
                    let artists: Vec<String> =
                        track.artists.iter().map(|a| a.name.clone()).collect();
                    let duration_ms = track.duration.num_milliseconds();
                    let minutes = duration_ms / 60_000;
                    let seconds = (duration_ms % 60_000) / 1000;

                    let thumbnail_url = track.album.images.first().map(|img| img.url.clone());

                    let url = track
                        .id
                        .as_ref()
                        .map(|id| format!("https://open.spotify.com/track/{}", id.id()))
                        .unwrap_or_default();

                    Track {
                        title: track.name,
                        artist: artists.join(", "),
                        url,
                        source: TrackSource::Spotify,
                        duration: Some(format!("{minutes}:{seconds:02}")),
                        thumbnail_url,
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub async fn get_track(&self, id: &str) -> Option<Track> {
        let track_id = TrackId::from_id(id).ok()?;
        let full_track = self.client.track(track_id, None).await.ok()?;

        let artists: Vec<String> = full_track.artists.iter().map(|a| a.name.clone()).collect();
        let duration_ms = full_track.duration.num_milliseconds();
        let minutes = duration_ms / 60_000;
        let seconds = (duration_ms % 60_000) / 1000;

        let thumbnail_url = full_track.album.images.first().map(|img| img.url.clone());

        let url = format!("https://open.spotify.com/track/{id}");

        Some(Track {
            title: full_track.name,
            artist: artists.join(", "),
            url,
            source: TrackSource::Spotify,
            duration: Some(format!("{minutes}:{seconds:02}")),
            thumbnail_url,
        })
    }

    pub async fn get_playlist_tracks(&self, id: &str) -> Vec<Track> {
        let playlist_id = match rspotify::model::PlaylistId::from_id(id) {
            Ok(id) => id,
            Err(_) => return Vec::new(),
        };

        let stream = self.client.playlist_items(playlist_id, None, None);
        futures::pin_mut!(stream);

        let mut tracks = Vec::new();
        while let Ok(Some(item)) = stream.try_next().await {
            if let Some(PlayableItem::Track(full_track)) = item.track {
                let artists: Vec<String> =
                    full_track.artists.iter().map(|a| a.name.clone()).collect();
                let duration_ms = full_track.duration.num_milliseconds();
                let minutes = duration_ms / 60_000;
                let seconds = (duration_ms % 60_000) / 1000;

                let thumbnail_url = full_track.album.images.first().map(|img| img.url.clone());

                let url = full_track
                    .id
                    .as_ref()
                    .map(|id| format!("https://open.spotify.com/track/{}", id.id()))
                    .unwrap_or_default();

                tracks.push(Track {
                    title: full_track.name,
                    artist: artists.join(", "),
                    url,
                    source: TrackSource::Spotify,
                    duration: Some(format!("{minutes}:{seconds:02}")),
                    thumbnail_url,
                });
            }
        }
        tracks
    }

    pub async fn get_playlist_name(&self, id: &str) -> Option<String> {
        let playlist_id = PlaylistId::from_id(id).ok()?;
        let playlist = self
            .client
            .playlist(playlist_id, None, None)
            .await
            .ok()?;
        Some(playlist.name)
    }

    pub async fn get_album_name(&self, id: &str) -> Option<String> {
        let album_id = AlbumId::from_id(id).ok()?;
        let album = self.client.album(album_id, None).await.ok()?;
        Some(album.name)
    }

    pub async fn get_album_tracks(&self, id: &str) -> Vec<Track> {
        let album_id = match AlbumId::from_id(id) {
            Ok(id) => id,
            Err(_) => return Vec::new(),
        };

        let stream = self.client.album_track(album_id, None);
        futures::pin_mut!(stream);

        let mut tracks = Vec::new();
        while let Ok(Some(track)) = stream.try_next().await {
            tracks.push(self.simplified_track_to_track(&track, id));
        }
        tracks
    }

    fn simplified_track_to_track(&self, track: &SimplifiedTrack, album_id: &str) -> Track {
        let artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
        let duration_ms = track.duration.num_milliseconds();
        let minutes = duration_ms / 60_000;
        let seconds = (duration_ms % 60_000) / 1000;

        let url = track
            .id
            .as_ref()
            .map(|id| format!("https://open.spotify.com/track/{}", id.id()))
            .unwrap_or_default();

        // Album tracks don't carry album images; use album URL as fallback
        let _ = album_id;

        Track {
            title: track.name.clone(),
            artist: artists.join(", "),
            url,
            source: TrackSource::Spotify,
            duration: Some(format!("{minutes}:{seconds:02}")),
            thumbnail_url: None,
        }
    }
}
