use std::collections::VecDeque;

use super::track::Track;

#[derive(Debug, Default)]
pub struct MusicQueue {
    tracks: VecDeque<Track>,
}

impl MusicQueue {
    pub fn push(&mut self, track: Track) {
        self.tracks.push_back(track);
    }

    pub fn pop(&mut self) -> Option<Track> {
        self.tracks.pop_front()
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    pub fn list(&self) -> &VecDeque<Track> {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}
