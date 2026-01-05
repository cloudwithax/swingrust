//! Playlist model

use serde::{Deserialize, Serialize};

/// Playlist settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaylistSettings {
    #[serde(default)]
    pub has_gif: bool,
    #[serde(default = "default_banner_pos")]
    pub banner_pos: i32,
    #[serde(default)]
    pub square_img: bool,
    #[serde(default)]
    pub pinned: bool,
}

fn default_banner_pos() -> i32 {
    50
}

/// A playlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    /// Database ID
    pub id: i64,
    /// Playlist name
    pub name: String,
    /// Image path (nullable)
    #[serde(default)]
    pub image: Option<String>,
    /// Last updated timestamp
    pub last_updated: String,
    /// List of track hashes
    #[serde(default)]
    pub trackhashes: Vec<String>,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
    /// Playlist settings
    #[serde(default)]
    pub settings: PlaylistSettings,
    /// Owner user ID
    #[serde(default)]
    pub userid: Option<i64>,
    /// Thumbnail path (computed)
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub thumb: String,
    /// Track count (computed)
    #[serde(default)]
    pub count: i32,
    /// Duration in seconds (computed)
    #[serde(default)]
    pub duration: i32,
    /// Is pinned (computed from settings)
    #[serde(default)]
    pub pinned: bool,
    /// Has custom image
    #[serde(default)]
    pub has_image: bool,
    /// Images for display (first 4 album arts)
    #[serde(default)]
    pub images: Vec<String>,
    /// Is editable by current user
    #[serde(default)]
    pub is_editable: bool,
}

impl Playlist {
    /// Create a new playlist
    pub fn new(name: String, userid: Option<i64>) -> Self {
        Self {
            id: 0,
            name,
            image: None,
            last_updated: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            trackhashes: Vec::new(),
            extra: serde_json::Value::Null,
            settings: PlaylistSettings::default(),
            userid,
            thumb: String::new(),
            count: 0,
            duration: 0,
            pinned: false,
            has_image: false,
            images: Vec::new(),
            is_editable: false,
        }
    }

    /// Initialize computed fields
    pub fn init(&mut self) {
        self.count = self.trackhashes.len() as i32;
        self.pinned = self.settings.pinned;
        self.has_image = self.image.is_some();

        if let Some(ref img) = self.image {
            self.thumb = img.clone();
        }
    }

    /// Clear trackhashes (for API response)
    pub fn clear_trackhashes(&mut self) {
        self.trackhashes.clear();
    }

    /// Create from database row
    pub fn from_db_row(
        id: i64,
        name: String,
        image: Option<String>,
        last_updated: String,
        trackhashes: Vec<String>,
        settings: PlaylistSettings,
        userid: Option<i64>,
        extra: serde_json::Value,
    ) -> Self {
        let mut playlist = Self {
            id,
            name,
            image,
            last_updated,
            trackhashes,
            extra,
            settings,
            userid,
            thumb: String::new(),
            count: 0,
            duration: 0,
            pinned: false,
            has_image: false,
            images: Vec::new(),
            is_editable: false,
        };
        playlist.init();
        playlist
    }
}

impl Default for Playlist {
    fn default() -> Self {
        Self::new(String::new(), None)
    }
}

impl PartialEq for Playlist {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Playlist {}
