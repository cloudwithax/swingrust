//! User configuration for SwingMusic
//!
//! This module handles user-configurable settings stored in settings.json.

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use super::Paths;

static USER_CONFIG: OnceCell<Arc<RwLock<UserConfig>>> = OnceCell::new();

/// User configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConfig {
    /// Server ID used for JWT secret
    #[serde(default)]
    pub server_id: String,

    /// Show user list on login page
    #[serde(default = "default_true")]
    pub users_on_login: bool,

    /// Root directories to scan for music
    #[serde(default)]
    pub root_dirs: Vec<String>,

    /// Directories to exclude from scanning
    #[serde(default)]
    pub exclude_dirs: Vec<String>,

    /// Artist name separators
    #[serde(default = "default_artist_separators")]
    pub artist_separators: HashSet<String>,

    /// Artists to ignore when splitting (e.g., "AC/DC")
    #[serde(default = "default_artist_split_ignore_list")]
    pub artist_split_ignore_list: HashSet<String>,

    /// Genre separators
    #[serde(default = "default_genre_separators")]
    pub genre_separators: HashSet<String>,

    /// Extract featured artists from track titles
    #[serde(default = "default_true")]
    pub extract_featured_artists: bool,

    /// Remove "(prod. by X)" from track titles
    #[serde(default = "default_true")]
    pub remove_prod_by: bool,

    /// Remove remaster info from track titles
    #[serde(default = "default_true")]
    pub remove_remaster_info: bool,

    /// Merge albums with same title
    #[serde(default)]
    pub merge_albums: bool,

    /// Clean album titles
    #[serde(default = "default_true")]
    pub clean_album_title: bool,

    /// Show albums as singles
    #[serde(default)]
    pub show_albums_as_singles: bool,

    /// Enable periodic scans
    #[serde(default)]
    pub enable_periodic_scans: bool,

    /// Scan interval in minutes
    #[serde(default = "default_scan_interval")]
    pub scan_interval: u32,

    /// Enable file watching
    #[serde(default)]
    pub enable_watchdog: bool,

    /// Show playlists in folder view
    #[serde(default)]
    pub show_playlists_in_folder_view: bool,

    /// Enable plugins
    #[serde(default = "default_true")]
    pub enable_plugins: bool,

    /// Last.fm API key
    #[serde(default = "default_lastfm_api_key")]
    pub lastfm_api_key: String,

    /// Last.fm API secret
    #[serde(default = "default_lastfm_api_secret")]
    pub lastfm_api_secret: String,

    /// Last.fm session keys per user
    #[serde(default)]
    pub lastfm_session_keys: std::collections::HashMap<String, String>,

    /// Enable guest user
    #[serde(default)]
    pub enable_guest: bool,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            server_id: String::new(),
            users_on_login: true,
            root_dirs: Vec::new(),
            exclude_dirs: Vec::new(),
            artist_separators: default_artist_separators(),
            artist_split_ignore_list: HashSet::new(),
            genre_separators: default_genre_separators(),
            extract_featured_artists: true,
            remove_prod_by: true,
            remove_remaster_info: true,
            merge_albums: false,
            clean_album_title: true,
            show_albums_as_singles: false,
            enable_periodic_scans: false,
            scan_interval: 10,
            enable_watchdog: false,
            show_playlists_in_folder_view: false,
            enable_plugins: true,
            lastfm_api_key: default_lastfm_api_key(),
            lastfm_api_secret: default_lastfm_api_secret(),
            lastfm_session_keys: std::collections::HashMap::new(),
            enable_guest: false,
        }
    }
}

impl UserConfig {
    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let paths = Paths::get()?;
        let settings_path = paths.settings_path();

        if settings_path.exists() {
            let content =
                std::fs::read_to_string(&settings_path).context("Failed to read settings file")?;
            let mut config: UserConfig =
                serde_json::from_str(&content).context("Failed to parse settings file")?;

            // Ensure essential defaults are present (fix for existing configs)
            config.artist_separators.insert(", ".to_string());
            config
                .artist_split_ignore_list
                .insert("tyler, the creator".to_string());

            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let paths = Paths::get()?;
        let settings_path = paths.settings_path();

        let content = serde_json::to_string_pretty(self).context("Failed to serialize settings")?;
        std::fs::write(&settings_path, content).context("Failed to write settings file")?;

        Ok(())
    }

    /// Get the global config instance
    pub fn global() -> Arc<RwLock<UserConfig>> {
        USER_CONFIG
            .get_or_init(|| {
                let config = UserConfig::load().unwrap_or_default();
                Arc::new(RwLock::new(config))
            })
            .clone()
    }

    /// Update a config value and save
    pub fn update<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Self),
    {
        f(self);
        self.save()
    }

    /// Load artist split ignore list from file
    pub fn load_artist_split_ignore_list(&mut self) -> Result<()> {
        let paths = Paths::get()?;
        let ignore_file = paths.assets_dir().join("artist_split_ignore.txt");

        if ignore_file.exists() {
            let content = std::fs::read_to_string(&ignore_file)?;
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    self.artist_split_ignore_list.insert(trimmed.to_lowercase());
                }
            }
        }

        Ok(())
    }

    /// Check if a path is within root directories
    pub fn is_path_in_root_dirs(&self, path: &Path) -> bool {
        self.root_dirs.iter().any(|root| {
            let root_path = Path::new(root);
            path.starts_with(root_path)
        })
    }

    /// Get the Last.fm session key for a user
    pub fn get_lastfm_session_key(&self, user_id: &str) -> Option<&String> {
        self.lastfm_session_keys.get(user_id)
    }

    /// Set the Last.fm session key for a user
    pub fn set_lastfm_session_key(&mut self, user_id: String, session_key: String) {
        self.lastfm_session_keys.insert(user_id, session_key);
    }

    /// Remove the Last.fm session key for a user
    pub fn remove_lastfm_session_key(&mut self, user_id: &str) {
        self.lastfm_session_keys.remove(user_id);
    }
}

// Default value functions for serde

fn default_true() -> bool {
    true
}

fn default_artist_separators() -> HashSet<String> {
    [";".to_string(), "/".to_string(), ", ".to_string()]
        .into_iter()
        .collect()
}
// the smart detector now handles most patterns automatically, so the default is empty.
// users can still add manual overrides here for edge cases the heuristics miss.
fn default_artist_split_ignore_list() -> HashSet<String> {
    HashSet::new()
}


fn default_genre_separators() -> HashSet<String> {
    // note: intentionally not including "&" as many genres contain it (R&B, Drum & Bass, etc.)
    ["/".to_string(), ";".to_string()]
        .into_iter()
        .collect()
}

fn default_scan_interval() -> u32 {
    10
}

fn default_lastfm_api_key() -> String {
    // upstream default api key
    "0553005e93f9a4b4819d835182181806".to_string()
}

fn default_lastfm_api_secret() -> String {
    // upstream default api secret
    "5e5306fbf3e8e3bc92f039b6c6c4bd4e".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = UserConfig::default();
        assert!(config.users_on_login);
        assert!(config.extract_featured_artists);
        assert!(config.artist_separators.contains(";"));
        assert!(config.artist_separators.contains("/"));
        assert!(config.artist_separators.contains(", "));
        // note: artist_split_ignore_list is now empty by default since the smart
        // detector handles patterns like "tyler, the creator" automatically
        assert!(config.artist_split_ignore_list.is_empty());
    }

    #[test]
    fn test_serialization() {
        let config = UserConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: UserConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.users_on_login, deserialized.users_on_login);
    }
}
