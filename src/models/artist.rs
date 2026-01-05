//! Artist model

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::GenreRef;

/// An artist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    /// Database ID
    #[serde(default)]
    pub id: i64,
    /// Artist name
    pub name: String,
    /// Unique artist hash
    pub artisthash: String,
    /// Number of albums
    #[serde(default)]
    pub albumcount: i32,
    /// Number of tracks
    #[serde(default)]
    pub trackcount: i32,
    /// Total duration in seconds
    #[serde(default)]
    pub duration: i32,
    /// Creation date (Unix timestamp)
    #[serde(default)]
    pub created_date: i64,
    /// Most recent release date
    #[serde(default)]
    pub date: i64,
    /// Genres
    #[serde(default)]
    pub genres: Vec<GenreRef>,
    /// List of genre hashes
    #[serde(default)]
    pub genrehashes: Vec<String>,
    /// Last played timestamp
    #[serde(default)]
    pub lastplayed: i64,
    /// Play count
    #[serde(default)]
    pub playcount: i32,
    /// Total play duration
    #[serde(default)]
    pub playduration: i32,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
    /// Dominant color from image
    #[serde(default)]
    pub color: String,
    /// Image path
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub image: String,
    /// Search score
    #[serde(skip_serializing, default)]
    pub score: f32,
    /// User IDs who favorited this artist
    #[serde(default)]
    pub fav_userids: HashSet<i64>,
    /// Help text (for display)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub help_text: String,
}

impl Artist {
    /// Create a new artist
    pub fn new(name: String, artisthash: String) -> Self {
        Self {
            id: -1,
            name,
            artisthash,
            albumcount: 0,
            trackcount: 0,
            duration: 0,
            created_date: 0,
            date: 0,
            genres: Vec::new(),
            genrehashes: Vec::new(),
            lastplayed: 0,
            playcount: 0,
            playduration: 0,
            extra: serde_json::Value::Null,
            color: String::new(),
            image: String::new(),
            score: 0.0,
            fav_userids: HashSet::new(),
            help_text: String::new(),
        }
    }

    /// Check if the artist is a favorite for the given user
    pub fn is_favorite(&self, user_id: i64) -> bool {
        self.fav_userids.contains(&user_id)
    }

    /// Toggle favorite status for a user

    /// Get genres as a vector of strings
    pub fn genre_names(&self) -> Vec<String> {
        self.genres.iter().map(|g| g.name.clone()).collect()
    }

    pub fn toggle_favorite(&mut self, user_id: i64) -> bool {
        if self.fav_userids.contains(&user_id) {
            self.fav_userids.remove(&user_id);
            false
        } else {
            self.fav_userids.insert(user_id);
            true
        }
    }

    /// Generate the image path
    pub fn set_image(&mut self) {
        self.image = format!("{}.webp", self.artisthash);
    }

    /// Compute genre hashes from genres list
    pub fn compute_genrehashes(&mut self) {
        self.genrehashes = self.genres.iter().map(|g| g.genrehash.clone()).collect();
    }
}

impl Default for Artist {
    fn default() -> Self {
        Self::new(String::new(), String::new())
    }
}

impl PartialEq for Artist {
    fn eq(&self, other: &Self) -> bool {
        self.artisthash == other.artisthash
    }
}

impl Eq for Artist {}

impl std::hash::Hash for Artist {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.artisthash.hash(state);
    }
}

/// Reference to an artist (used in JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistRef {
    pub name: String,
    #[serde(default)]
    pub artisthash: String,
    #[serde(default)]
    pub image: String,
}

impl ArtistRef {
    pub fn new(name: String) -> Self {
        let artisthash = crate::utils::hashing::create_hash(&[&name], true);
        Self {
            name,
            artisthash,
            image: String::new(),
        }
    }

    pub fn with_hash(name: String, artisthash: String) -> Self {
        Self {
            name,
            artisthash,
            image: String::new(),
        }
    }
}

/// Similar artist entry from Last.fm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarArtist {
    pub artisthash: String,
    pub name: String,
    #[serde(default)]
    pub weight: f32,
    #[serde(default)]
    pub listeners: i64,
    #[serde(default)]
    pub scrobbles: i64,
}

impl SimilarArtist {
    pub fn new(name: String) -> Self {
        let artisthash = crate::utils::hashing::create_hash(&[&name], true);
        Self {
            artisthash,
            name,
            weight: 0.0,
            listeners: 0,
            scrobbles: 0,
        }
    }
}

/// Entry in the similar artists table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarArtistEntry {
    pub id: i64,
    pub artisthash: String,
    pub similar: Vec<SimilarArtist>,
}

impl SimilarArtistEntry {
    pub fn new(artisthash: String, similar: Vec<SimilarArtist>) -> Self {
        Self {
            id: 0,
            artisthash,
            similar,
        }
    }

    /// Get a set of similar artist hashes
    pub fn get_similar_hashes(&self) -> HashSet<String> {
        self.similar.iter().map(|s| s.artisthash.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artist_favorite() {
        let mut artist = Artist::new("Test Artist".into(), "hash".into());
        assert!(!artist.is_favorite(1));

        assert!(artist.toggle_favorite(1));
        assert!(artist.is_favorite(1));
    }

    #[test]
    fn test_artist_ref() {
        let artist_ref = ArtistRef::new("Test Artist".into());
        assert!(!artist_ref.artisthash.is_empty());
    }
}
