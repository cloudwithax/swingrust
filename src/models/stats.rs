//! Statistics models

use serde::{Deserialize, Serialize};

use crate::models::mix::MixSourceType;

/// A track log entry (scrobble)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackLog {
    /// Database ID
    pub id: i64,
    /// Track hash
    pub trackhash: String,
    /// Play timestamp
    pub timestamp: i64,
    /// Play duration in seconds
    pub duration: i32,
    /// Source of play
    pub source: String,
    /// User ID
    pub userid: i64,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
    /// Parsed source type
    #[serde(skip)]
    pub source_type: Option<MixSourceType>,
    /// Parsed source ID
    #[serde(skip)]
    pub source_id: Option<String>,
}

impl TrackLog {
    pub fn new(
        trackhash: String,
        timestamp: i64,
        duration: i32,
        source: String,
        userid: i64,
    ) -> Self {
        let (source_type, source_id) = Self::parse_source(&source);
        Self {
            id: 0,
            trackhash,
            timestamp,
            duration,
            source,
            userid,
            extra: serde_json::Value::Null,
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

impl Default for TrackLog {
    fn default() -> Self {
        Self::new(String::new(), 0, 0, String::new(), 0)
    }
}
