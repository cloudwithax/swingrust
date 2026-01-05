//! Folder model

use serde::{Deserialize, Serialize};

/// A folder in the music library
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Folder {
    /// Folder name
    pub name: String,
    /// Full path
    pub path: String,
    /// Is symbolic link
    #[serde(default)]
    pub is_sym: bool,
    /// Number of tracks in this folder
    #[serde(default)]
    pub trackcount: i32,
}

impl Folder {
    /// Create a new folder
    pub fn new(name: String, path: String) -> Self {
        Self {
            name,
            path,
            is_sym: false,
            trackcount: 0,
        }
    }

    /// Create a folder with track count
    pub fn with_trackcount(name: String, path: String, trackcount: i32) -> Self {
        Self {
            name,
            path,
            is_sym: false,
            trackcount,
        }
    }

    /// Create a folder from a path
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_string();
        let path_str = path.to_string_lossy().to_string();
        let is_sym = path.is_symlink();

        Some(Self {
            name,
            path: path_str,
            is_sym,
            trackcount: 0,
        })
    }
}

impl Default for Folder {
    fn default() -> Self {
        Self::new(String::new(), String::new())
    }
}

impl std::fmt::Display for Folder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path)
    }
}
