//! Last.fm related models

use serde::{Deserialize, Serialize};

/// Last.fm artist info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastfmArtist {
    pub name: String,
    pub mbid: Option<String>,
    pub url: String,
    pub listeners: i64,
    pub playcount: i64,
}

impl Default for LastfmArtist {
    fn default() -> Self {
        Self {
            name: String::new(),
            mbid: None,
            url: String::new(),
            listeners: 0,
            playcount: 0,
        }
    }
}
