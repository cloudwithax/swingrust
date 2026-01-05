//! Album model

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::{ArtistRefItem, GenreRef, Track};
use crate::config::UserConfig;
use crate::utils::hashing::create_hash;

/// Album type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AlbumType {
    #[default]
    Album,
    Single,
    Ep,
    Compilation,
    Soundtrack,
    #[serde(rename = "live album")]
    LiveAlbum,
}

impl AlbumType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlbumType::Album => "album",
            AlbumType::Single => "single",
            AlbumType::Ep => "ep",
            AlbumType::Compilation => "compilation",
            AlbumType::Soundtrack => "soundtrack",
            AlbumType::LiveAlbum => "live album",
        }
    }
}

impl std::fmt::Display for AlbumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An album
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    /// Database ID
    #[serde(default)]
    pub id: i64,
    /// Album artists
    #[serde(default)]
    pub albumartists: Vec<ArtistRefItem>,
    /// Unique album hash
    pub albumhash: String,
    /// List of artist hashes
    #[serde(default)]
    pub artisthashes: Vec<String>,
    /// Base title (without version info)
    #[serde(default)]
    pub base_title: String,
    /// Dominant color from artwork
    #[serde(default)]
    pub color: String,
    /// Creation date (Unix timestamp)
    #[serde(default)]
    pub created_date: i64,
    /// Release date (Unix timestamp)
    #[serde(default)]
    pub date: i64,
    /// Total duration in seconds
    #[serde(default)]
    pub duration: i32,
    /// Genres
    #[serde(default)]
    pub genres: Vec<GenreRef>,
    /// List of genre hashes
    #[serde(default)]
    pub genrehashes: Vec<String>,
    /// Original title (before processing)
    #[serde(default)]
    pub og_title: String,
    /// Processed title
    pub title: String,
    /// Number of tracks
    #[serde(default)]
    pub trackcount: i32,
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
    /// Album type
    #[serde(default, rename = "type")]
    pub album_type: AlbumType,
    /// Path hash (for image lookup)
    #[serde(default)]
    pub pathhash: String,
    /// Image path
    #[serde(default)]
    pub image: String,
    /// Album versions (deluxe, remaster, etc.)
    #[serde(default)]
    pub versions: Vec<String>,
    /// Search score
    #[serde(skip_serializing, default)]
    pub score: f32,
    /// User IDs who favorited this album
    #[serde(default)]
    pub fav_userids: HashSet<i64>,
    /// Weak hash (for matching)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub weakhash: String,
    /// Help text (for display)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub help_text: String,
}

impl Album {
    /// Create a new album with default values
    pub fn new(albumhash: String, title: String) -> Self {
        Self {
            id: -1,
            albumartists: Vec::new(),
            albumhash,
            artisthashes: Vec::new(),
            base_title: String::new(),
            color: String::new(),
            created_date: 0,
            date: 0,
            duration: 0,
            genres: Vec::new(),
            genrehashes: Vec::new(),
            og_title: title.clone(),
            title,
            trackcount: 0,
            lastplayed: 0,
            playcount: 0,
            playduration: 0,
            extra: serde_json::Value::Null,
            album_type: AlbumType::Album,
            pathhash: String::new(),
            image: String::new(),
            versions: Vec::new(),
            score: 0.0,
            fav_userids: HashSet::new(),
            weakhash: String::new(),
            help_text: String::new(),
        }
    }

    /// Get album artist as a comma-separated string
    pub fn albumartist(&self) -> String {
        self.albumartists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Get count (trackcount)
    pub fn count(&self) -> i32 {
        self.trackcount
    }

    /// Get genres as a vector of strings
    pub fn genre_names(&self) -> Vec<String> {
        self.genres.iter().map(|g| g.name.clone()).collect()
    }

    /// Check if the album is a favorite for the given user
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

    /// Generate the image path
    pub fn set_image(&mut self) {
        self.image = format!("{}.webp", self.albumhash);
    }

    /// Extract version information from title
    pub fn set_versions(&mut self) {
        use crate::utils::parsers::get_album_versions;
        self.versions = get_album_versions(&self.title);
    }

    /// Extract base title (without version info)
    pub fn set_base_title(&mut self) {
        use crate::utils::parsers::get_base_album_title;
        self.base_title = get_base_album_title(&self.title);
    }

    /// Determine album type based on tracks
    pub fn set_type(&mut self, tracks: &[Track]) {
        self.album_type = self.determine_type(tracks);
    }

    /// Determine the album type
    fn determine_type(&self, tracks: &[Track]) -> AlbumType {
        let show_as_singles = UserConfig::global().read().show_albums_as_singles;

        if self.is_single(tracks, show_as_singles) {
            return AlbumType::Single;
        }
        if self.is_soundtrack() {
            return AlbumType::Soundtrack;
        }
        if self.is_live_album() {
            return AlbumType::LiveAlbum;
        }
        if self.is_compilation() {
            return AlbumType::Compilation;
        }
        if self.is_ep() {
            return AlbumType::Ep;
        }
        AlbumType::Album
    }

    /// Check if this is a soundtrack
    fn is_soundtrack(&self) -> bool {
        let title_lower = self.og_title.to_lowercase();
        title_lower.contains("motion picture") || title_lower.contains("soundtrack")
    }

    /// Check if this is a compilation
    fn is_compilation(&self) -> bool {
        let artists = self
            .albumartists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<String>()
            .to_lowercase();

        if artists.contains("various artists") {
            return true;
        }

        let substrings = [
            "the essential",
            "best of",
            "greatest hits",
            "#1 hits",
            "number ones",
            "super hits",
            "collection",
            "anthology",
            "great hits",
            "biggest hits",
            "the hits",
            "the ultimate",
            "compilation",
        ];

        let title_lower = self.title.to_lowercase();
        substrings.iter().any(|s| title_lower.contains(s))
    }

    /// Check if this is a live album
    fn is_live_album(&self) -> bool {
        let title_lower = self.og_title.to_lowercase();
        let keywords = [
            "live from",
            "live at",
            "live in",
            "live on",
            "mtv unplugged",
        ];
        keywords.iter().any(|k| title_lower.contains(k))
    }

    /// Check if this is an EP
    fn is_ep(&self) -> bool {
        self.title.trim_end().ends_with(" EP")
    }

    /// Check if this is a single
    fn is_single(&self, tracks: &[Track], show_as_singles: bool) -> bool {
        let keywords = ["single version", "- single"];
        let og = self.og_title.to_lowercase();
        if keywords.iter().any(|k| og.contains(k)) {
            return true;
        }

        if show_as_singles && self.trackcount == 1 {
            return true;
        }

        if tracks.len() == 1 {
            let track = &tracks[0];
            let track_hash = create_hash(&[track.title.as_str()], false);
            let title_hash = create_hash(&[self.title.as_str()], false);
            let og_title_hash = create_hash(&[self.og_title.as_str()], false);

            if track_hash == title_hash || track_hash == og_title_hash {
                return true;
            }
        }

        false
    }

    /// Initialize computed fields
    pub fn init(&mut self, tracks: &[Track]) {
        self.set_image();
        self.set_base_title();
        self.set_versions();
        self.set_type(tracks);

        // Compute artisthashes
        self.artisthashes = self
            .albumartists
            .iter()
            .map(|a| a.artisthash.clone())
            .collect();

        // Compute genrehashes
        self.genrehashes = self.genres.iter().map(|g| g.genrehash.clone()).collect();
    }
}

impl Default for Album {
    fn default() -> Self {
        Self::new(String::new(), String::new())
    }
}

impl PartialEq for Album {
    fn eq(&self, other: &Self) -> bool {
        self.albumhash == other.albumhash
    }
}

impl Eq for Album {}

impl std::hash::Hash for Album {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.albumhash.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_album_type_detection() {
        let album = Album::new("hash".into(), "Greatest Hits".into());
        assert!(album.is_compilation());

        let album = Album::new("hash".into(), "Live at Wembley".into());
        assert!(album.is_live_album());

        let album = Album::new("hash".into(), "Demo EP".into());
        assert!(album.is_ep());
    }

    #[test]
    fn test_album_favorite() {
        let mut album = Album::new("hash".into(), "Test".into());
        assert!(!album.is_favorite(1));

        assert!(album.toggle_favorite(1));
        assert!(album.is_favorite(1));
    }
}
