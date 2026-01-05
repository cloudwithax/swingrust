//! Track model

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::{ArtistRefItem, GenreRef};
use crate::utils::hashing::create_hash;

/// A music track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Database ID
    pub id: i64,
    /// Album name
    pub album: String,
    /// Album artists
    #[serde(default)]
    pub albumartists: Vec<ArtistRefItem>,
    /// Album hash
    pub albumhash: String,
    /// Track artists
    #[serde(default)]
    pub artists: Vec<ArtistRefItem>,
    /// Bitrate in kbps
    pub bitrate: i32,
    /// Copyright info
    #[serde(default)]
    pub copyright: Option<String>,
    /// Release date (Unix timestamp)
    #[serde(default)]
    pub date: i64,
    /// Disc number
    pub disc: i32,
    /// Duration in seconds
    pub duration: i32,
    /// File path
    pub filepath: String,
    /// Folder path
    pub folder: String,
    /// Genres
    #[serde(default)]
    pub genres: Vec<GenreRef>,
    /// Last modified timestamp
    pub last_mod: i64,
    /// Track title
    pub title: String,
    /// Track number
    pub track: i32,
    /// Unique track hash
    pub trackhash: String,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
    /// Last played timestamp
    #[serde(default)]
    pub lastplayed: i64,
    /// Play count
    #[serde(default)]
    pub playcount: i32,
    /// Total play duration in seconds
    #[serde(default)]
    pub playduration: i32,

    // Computed/transient fields
    /// Original album title (before processing)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub og_album: String,
    /// Original track title (before processing)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub og_title: String,
    /// List of artist hashes
    #[serde(default)]
    pub artisthashes: Vec<String>,
    /// List of genre hashes
    #[serde(default)]
    pub genrehashes: Vec<String>,
    /// Weak hash (without artists)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub weakhash: String,
    /// Position in queue
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pos: Option<i32>,
    /// Image path
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub image: String,
    /// Help text (for display)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub help_text: String,
    /// Search score
    #[serde(skip_serializing, default)]
    pub score: f32,
    /// Is explicit content
    #[serde(default)]
    pub explicit: bool,
    /// User IDs who favorited this track
    #[serde(default)]
    pub fav_userids: HashSet<i64>,
}

impl Track {
    /// Create a new track with default values
    pub fn new() -> Self {
        Self {
            id: 0,
            album: String::new(),
            albumartists: Vec::new(),
            albumhash: String::new(),
            artists: Vec::new(),
            bitrate: 0,
            copyright: None,
            date: 0,
            disc: 1,
            duration: 0,
            filepath: String::new(),
            folder: String::new(),
            genres: Vec::new(),
            last_mod: 0,
            title: String::new(),
            track: 0,
            trackhash: String::new(),
            extra: serde_json::Value::Null,
            lastplayed: 0,
            playcount: 0,
            playduration: 0,
            og_album: String::new(),
            og_title: String::new(),
            artisthashes: Vec::new(),
            genrehashes: Vec::new(),
            weakhash: String::new(),
            pos: None,
            image: String::new(),
            help_text: String::new(),
            score: 0.0,
            explicit: false,
            fav_userids: HashSet::new(),
        }
    }

    /// Get artist as a comma-separated string
    pub fn artist(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Get album artist as a comma-separated string
    pub fn albumartist(&self) -> String {
        self.albumartists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Get genre as a comma-separated string
    pub fn genre(&self) -> String {
        self.genres
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Get genres as a vector of strings
    pub fn genre_names(&self) -> Vec<String> {
        self.genres.iter().map(|g| g.name.clone()).collect()
    }

    /// Check if the track is a favorite for the given user
    pub fn is_favorite(&self, user_id: i64) -> bool {
        self.fav_userids.contains(&user_id)
    }

    /// Toggle favorite status for a user
    pub fn toggle_favorite(&mut self, user_id: i64) -> bool {
        if self.fav_userids.contains(&user_id) {
            self.fav_userids.remove(&user_id);
            false
        } else {
            self.fav_userids.insert(user_id);
            true
        }
    }

    /// Get the folder hash
    pub fn folderhash(&self) -> String {
        create_hash(&[&self.folder], false)
    }

    /// Generate the image path
    pub fn generate_image(&mut self) {
        let pathhash = create_hash(&[&self.folder], false);
        self.image = format!("{}.webp?pathhash={}", self.albumhash, pathhash);
    }

    /// Compute artist hashes from artists list
    pub fn compute_artisthashes(&mut self) {
        self.artisthashes = self.artists.iter().map(|a| a.artisthash.clone()).collect();
    }

    /// Compute genre hashes from genres list
    pub fn compute_genrehashes(&mut self) {
        self.genrehashes = self.genres.iter().map(|g| g.genrehash.clone()).collect();
    }

    /// Regenerate the track hash
    pub fn regenerate_trackhash(&mut self) {
        let artist_str: String = self.artists.iter().map(|a| a.name.as_str()).collect();
        self.trackhash = create_hash(&[&artist_str, &self.album, &self.title], true);
    }

    /// Get disc and track as a sortable position
    pub fn sort_position(&self) -> i32 {
        self.disc * 1000 + self.track
    }
}

impl Default for Track {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Track {
    fn eq(&self, other: &Self) -> bool {
        self.trackhash == other.trackhash
    }
}

impl Eq for Track {}

impl std::hash::Hash for Track {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.trackhash.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_favorite() {
        let mut track = Track::new();
        assert!(!track.is_favorite(1));

        assert!(track.toggle_favorite(1));
        assert!(track.is_favorite(1));

        assert!(!track.toggle_favorite(1));
        assert!(!track.is_favorite(1));
    }

    #[test]
    fn test_sort_position() {
        let mut track = Track::new();
        track.disc = 1;
        track.track = 5;
        assert_eq!(track.sort_position(), 1005);

        track.disc = 2;
        track.track = 3;
        assert_eq!(track.sort_position(), 2003);
    }
}
