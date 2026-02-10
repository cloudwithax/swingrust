//! Mix model

use serde::{Deserialize, Serialize};

/// A mix (personalized playlist)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mix {
    /// Database ID
    pub id: i64,
    /// Timestamp when the mix was created
    #[serde(default)]
    pub timestamp: i64,
    /// Mix ID (unique identifier)
    pub mixid: String,
    /// Mix title
    pub title: String,
    /// Description
    pub description: String,
    /// List of track hashes
    #[serde(default)]
    pub trackhashes: Vec<String>,
    /// Source hash (artist/album that inspired the mix)
    pub sourcehash: String,
    /// User ID
    pub userid: i64,
    /// Is saved by user
    #[serde(default)]
    pub saved: bool,
    /// Images for display
    #[serde(default)]
    pub images: Vec<String>,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Mix {
    /// Create a new mix
    pub fn new(
        mixid: String,
        title: String,
        description: String,
        trackhashes: Vec<String>,
        sourcehash: String,
        userid: i64,
    ) -> Self {
        Self {
            id: 0,
            timestamp: chrono::Utc::now().timestamp(),
            mixid,
            title,
            description,
            trackhashes,
            sourcehash,
            userid,
            saved: false,
            images: Vec::new(),
            extra: serde_json::Value::Null,
        }
    }

    /// Serialize with full track details
    pub fn to_full(&self, tracks: Vec<super::Track>) -> MixWithTracks {
        MixWithTracks {
            id: self.id,
            mixid: self.mixid.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            tracks,
            sourcehash: self.sourcehash.clone(),
            userid: self.userid,
            saved: self.saved,
            images: self.images.clone(),
            extra: self.extra.clone(),
        }
    }

    /// Create from database row
    pub fn from_db_row(
        id: i64,
        timestamp: i64,
        mixid: String,
        title: String,
        description: String,
        trackhashes: Vec<String>,
        sourcehash: String,
        userid: i64,
        saved: bool,
        images: Vec<String>,
        extra: serde_json::Value,
    ) -> Self {
        Self {
            id,
            timestamp,
            mixid,
            title,
            description,
            trackhashes,
            sourcehash,
            userid,
            saved,
            images,
            extra,
        }
    }
}

impl Default for Mix {
    fn default() -> Self {
        Self::new(
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
            String::new(),
            0,
        )
    }
}

/// Mix with full track objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixWithTracks {
    pub id: i64,
    pub mixid: String,
    pub title: String,
    pub description: String,
    pub tracks: Vec<super::Track>,
    pub sourcehash: String,
    pub userid: i64,
    pub saved: bool,
    pub images: Vec<String>,
    pub extra: serde_json::Value,
}

/// Source type for mix generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MixSourceType {
    Artist,
    Track,
    Album,
    Folder,
    Playlist,
    Favorite,
}

impl MixSourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MixSourceType::Artist => "ar",
            MixSourceType::Track => "tr",
            MixSourceType::Album => "al",
            MixSourceType::Folder => "fo",
            MixSourceType::Playlist => "pl",
            MixSourceType::Favorite => "favorite",
        }
    }

    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "ar" => Some(MixSourceType::Artist),
            "tr" => Some(MixSourceType::Track),
            "al" => Some(MixSourceType::Album),
            "fo" => Some(MixSourceType::Folder),
            "pl" => Some(MixSourceType::Playlist),
            "favorite" => Some(MixSourceType::Favorite),
            _ => None,
        }
    }
}

/// Track log for mix generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixTrackLog {
    pub trackhash: String,
    pub albumhash: String,
    pub artisthashes: Vec<String>,
    pub source: String,
    pub duration: i32,
    pub timestamp: i64,
    #[serde(default)]
    pub source_type: Option<MixSourceType>,
    #[serde(default)]
    pub source_id: Option<String>,
}

impl MixTrackLog {
    pub fn new(
        trackhash: String,
        albumhash: String,
        artisthashes: Vec<String>,
        source: String,
        duration: i32,
        timestamp: i64,
    ) -> Self {
        let (source_type, source_id) = Self::parse_source(&source);
        Self {
            trackhash,
            albumhash,
            artisthashes,
            source,
            duration,
            timestamp,
            source_type,
            source_id,
        }
    }

    fn parse_source(source: &str) -> (Option<MixSourceType>, Option<String>) {
        if source == "favorite" {
            return (Some(MixSourceType::Favorite), None);
        }

        if let Some((prefix, id)) = source.split_once(':') {
            let source_type = MixSourceType::from_prefix(prefix);
            return (source_type, Some(id.to_string()));
        }

        (None, None)
    }
}
