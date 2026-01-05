//! Path management for SwingMusic
//!
//! This module manages all filesystem paths used by the application.

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

static PATHS: OnceCell<Arc<Paths>> = OnceCell::new();

/// Manages all filesystem paths for the application
#[derive(Debug, Clone)]
pub struct Paths {
    /// Parent directory of config folder
    config_parent: PathBuf,
    /// Path to web client files
    client_path: PathBuf,
    /// Config directory path
    config_dir: PathBuf,
}

impl Paths {
    /// Initialize the paths singleton
    pub fn init(config: Option<PathBuf>, client: Option<PathBuf>) -> Result<Arc<Paths>> {
        let paths = PATHS.get_or_try_init(|| {
            let paths = Self::new(config, client)?;
            Ok::<_, anyhow::Error>(Arc::new(paths))
        })?;
        Ok(Arc::clone(paths))
    }

    /// Get the global paths instance
    pub fn get() -> Result<Arc<Paths>> {
        PATHS.get().map(Arc::clone).context("Paths not initialized")
    }

    fn new(config_override: Option<PathBuf>, client_override: Option<PathBuf>) -> Result<Self> {
        // Determine config parent directory
        let config_parent = if let Some(ref path) = config_override {
            path.clone()
        } else if let Ok(exe) = std::env::current_exe() {
            exe.parent().unwrap_or(Path::new(".")).to_path_buf()
        } else {
            directories::ProjectDirs::from("", "", "swingmusic")
                .map(|dirs| dirs.config_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };

        // Determine config directory name
        let config_dir_name = if is_home_dir(&config_parent) {
            ".swingmusic"
        } else {
            "swingmusic"
        };

        let config_dir = config_parent.join(config_dir_name);

        // Determine client path
        let client_path = client_override.unwrap_or_else(|| config_dir.join("client"));

        let paths = Self {
            config_parent,
            client_path,
            config_dir,
        };

        // Create directories
        paths.create_directories()?;

        Ok(paths)
    }

    fn create_directories(&self) -> Result<()> {
        // Create main config directory
        std::fs::create_dir_all(&self.config_dir)?;

        // Create subdirectories
        let subdirs = [
            "client",
            "assets",
            "plugins/lyrics",
            "images/artists/small",
            "images/artists/medium",
            "images/artists/large",
            "images/thumbnails/xsmall",
            "images/thumbnails/small",
            "images/thumbnails/medium",
            "images/thumbnails/large",
            "images/playlists",
            "images/mixes/original",
            "images/mixes/medium",
            "images/mixes/small",
            "backups",
        ];

        for subdir in subdirs {
            std::fs::create_dir_all(self.config_dir.join(subdir))?;
        }

        Ok(())
    }

    // ========== Getters ==========

    /// Get the config directory
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Get the config folder (alias for config_dir)
    pub fn config_folder(&self) -> &Path {
        self.config_dir()
    }

    /// Get the user database path (alias for userdata_db_path)
    pub fn user_db_path(&self) -> PathBuf {
        self.userdata_db_path()
    }

    /// Get the album images directory (alias for thumbnails_dir)
    pub fn album_images(&self, size: &str) -> PathBuf {
        self.thumbnails_dir(size)
    }

    /// Get the artist images directory (wrapper for artist_images_dir)
    pub fn artist_images(&self, size: &str) -> PathBuf {
        self.artist_images_dir(size)
    }

    /// Get the config parent directory
    pub fn config_parent(&self) -> &Path {
        &self.config_parent
    }

    /// Get the client path
    pub fn client_path(&self) -> &Path {
        &self.client_path
    }

    /// Get the main database path
    pub fn app_db_path(&self) -> PathBuf {
        self.config_dir.join("swingmusic.db")
    }

    /// Get the userdata database path
    pub fn userdata_db_path(&self) -> PathBuf {
        self.config_dir.join("userdata.db")
    }

    /// Get the settings file path
    pub fn settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// Get the assets directory
    pub fn assets_dir(&self) -> PathBuf {
        self.config_dir.join("assets")
    }

    /// Get the plugins directory
    pub fn plugins_dir(&self) -> PathBuf {
        self.config_dir.join("plugins")
    }

    /// Get the lyrics plugins directory
    pub fn lyrics_plugins_dir(&self) -> PathBuf {
        self.plugins_dir().join("lyrics")
    }

    /// Get the backups directory
    pub fn backups_dir(&self) -> PathBuf {
        self.config_dir.join("backups")
    }

    // ========== Image Paths ==========

    /// Get the images directory
    pub fn images_dir(&self) -> PathBuf {
        self.config_dir.join("images")
    }

    /// Get artist images directory for a specific size
    pub fn artist_images_dir(&self, size: &str) -> PathBuf {
        self.images_dir().join("artists").join(size)
    }

    /// Get thumbnail directory for a specific size
    pub fn thumbnails_dir(&self, size: &str) -> PathBuf {
        self.images_dir().join("thumbnails").join(size)
    }

    /// Get playlist images directory
    pub fn playlist_images_dir(&self) -> PathBuf {
        self.images_dir().join("playlists")
    }

    /// Get mix images directory for a specific size
    pub fn mix_images_dir(&self, size: &str) -> PathBuf {
        self.images_dir().join("mixes").join(size)
    }

    // ========== Path Helpers ==========

    /// Get the path for an album thumbnail
    pub fn get_thumbnail_path(&self, albumhash: &str, size: &str) -> PathBuf {
        self.thumbnails_dir(size)
            .join(format!("{}.webp", albumhash))
    }

    /// Get the path for an artist image
    pub fn get_artist_image_path(&self, artisthash: &str, size: &str) -> PathBuf {
        self.artist_images_dir(size)
            .join(format!("{}.webp", artisthash))
    }

    /// Get the path for a playlist image
    pub fn get_playlist_image_path(&self, playlist_id: i64) -> PathBuf {
        self.playlist_images_dir()
            .join(format!("{}.webp", playlist_id))
    }

    /// Get the path for a mix image
    pub fn get_mix_image_path(&self, mix_id: &str, size: &str) -> PathBuf {
        self.mix_images_dir(size).join(format!("{}.webp", mix_id))
    }
}

/// Check if a path is in the user's home directory
fn is_home_dir(path: &Path) -> bool {
    directories::UserDirs::new()
        .map(|dirs| path.starts_with(dirs.home_dir()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_paths_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = Some(temp_dir.path().to_path_buf());

        // Note: Can't use init() in tests due to OnceCell
        let paths = Paths::new(config, None).unwrap();

        assert!(paths.config_dir().exists());
        assert!(paths.thumbnails_dir("large").exists());
        assert!(paths.artist_images_dir("medium").exists());
    }
}
