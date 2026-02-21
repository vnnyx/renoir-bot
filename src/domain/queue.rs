use std::collections::VecDeque;

use super::track::Track;

#[derive(Debug, Default)]
pub struct MusicQueue {
    current: Option<Track>,
    tracks: VecDeque<Track>,
}

impl MusicQueue {
    pub fn push(&mut self, track: Track) {
        self.tracks.push_back(track);
    }

    pub fn pop(&mut self) -> Option<Track> {
        self.tracks.pop_front()
    }

    /// Pops the next track from the queue into `current`, returning a reference to it.
    pub fn advance(&mut self) -> Option<&Track> {
        self.current = self.tracks.pop_front();
        self.current.as_ref()
    }

    /// Returns a reference to the currently playing track.
    pub fn current(&self) -> Option<&Track> {
        self.current.as_ref()
    }

    /// Takes the current track out (used by skip to return the skipped track).
    pub fn take_current(&mut self) -> Option<Track> {
        self.current.take()
    }

    pub fn clear(&mut self) {
        self.current = None;
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
