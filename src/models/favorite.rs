//! Favorite model

use serde::{Deserialize, Serialize};

/// Favorite type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FavoriteType {
    Track,
    Album,
    Artist,
}

impl FavoriteType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FavoriteType::Track => "track",
            FavoriteType::Album => "album",
            FavoriteType::Artist => "artist",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "track" => Some(FavoriteType::Track),
            "album" => Some(FavoriteType::Album),
            "artist" => Some(FavoriteType::Artist),
            _ => None,
        }
    }
}

impl std::fmt::Display for FavoriteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A favorite entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Favorite {
    /// Database ID
    pub id: i64,
    /// Item hash (trackhash, albumhash, or artisthash)
    pub hash: String,
    /// Type of favorite
    #[serde(rename = "type")]
    pub favorite_type: FavoriteType,
    /// Timestamp when favorited
    pub timestamp: i64,
    /// User who favorited
    pub userid: i64,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Favorite {
    /// Create a new favorite
    pub fn new(hash: String, favorite_type: FavoriteType, userid: i64) -> Self {
        Self {
            id: 0,
            hash,
            favorite_type,
            timestamp: chrono::Utc::now().timestamp(),
            userid,
            extra: serde_json::Value::Null,
        }
    }

    /// Get the prefixed hash (for database storage)
    pub fn prefixed_hash(&self) -> String {
        format!("{}_{}", self.favorite_type.as_str(), self.hash)
    }

    /// Parse a prefixed hash
    pub fn parse_prefixed_hash(prefixed: &str) -> Option<(FavoriteType, String)> {
        let parts: Vec<&str> = prefixed.splitn(2, '_').collect();
        if parts.len() != 2 {
            return None;
        }

        let fav_type = FavoriteType::from_str(parts[0])?;
        Some((fav_type, parts[1].to_string()))
    }
}

impl Default for Favorite {
    fn default() -> Self {
        Self::new(String::new(), FavoriteType::Track, 0)
    }
}
