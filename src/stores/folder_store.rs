//! Folder store - in-memory folder storage for browsing

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::config::UserConfig;
use crate::models::Folder;
use crate::stores::TrackStore;

/// Global folder store instance
static FOLDER_STORE: OnceLock<Arc<FolderStore>> = OnceLock::new();

/// In-memory store for folders
pub struct FolderStore {
    /// All folders by path
    folders: RwLock<HashMap<String, Folder>>,
    /// Root directories
    root_dirs: RwLock<Vec<String>>,
}

impl FolderStore {
    /// Get or initialize the global folder store
    pub fn get() -> Arc<FolderStore> {
        FOLDER_STORE
            .get_or_init(|| {
                Arc::new(FolderStore {
                    folders: RwLock::new(HashMap::new()),
                    root_dirs: RwLock::new(Vec::new()),
                })
            })
            .clone()
    }

    /// Set root directories
    pub fn set_root_dirs(&self, dirs: Vec<String>) {
        *self.root_dirs.write().unwrap() = dirs;
    }

    /// Get root directories
    pub fn get_root_dirs(&self) -> Vec<String> {
        self.root_dirs.read().unwrap().clone()
    }

    /// Load folders from track paths
    pub fn load_from_paths(&self, track_folders: Vec<String>, root_dirs: &[String]) {
        let mut folder_map = self.folders.write().unwrap();
        folder_map.clear();

        let mut all_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Add all track folders
        for folder in &track_folders {
            all_paths.insert(folder.clone());
        }

        // Also add parent directories up to root
        for folder in &track_folders {
            let mut current = std::path::PathBuf::from(folder);
            while let Some(parent) = current.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                if root_dirs.iter().any(|r| parent_str.starts_with(r)) {
                    all_paths.insert(parent_str.clone());
                    current = parent.to_path_buf();
                } else {
                    break;
                }
            }
        }

        // Build folder entries
        for path in all_paths {
            let folder = Self::make_folder(&path, &track_folders);
            folder_map.insert(path, folder);
        }

        // Update root dirs
        *self.root_dirs.write().unwrap() = root_dirs.to_vec();
    }

    /// Load folders from the track store and user config
    pub async fn load_filepaths() -> Result<()> {
        let config = UserConfig::load()?;
        let track_folders: Vec<String> = TrackStore::get()
            .get_all()
            .into_iter()
            .map(|t| t.folder)
            .collect();

        FolderStore::get().load_from_paths(track_folders, &config.root_dirs);
        Ok(())
    }

    /// Count tracks contained in the provided folder paths
    pub fn count_tracks_containing_paths(&self, paths: &[String]) -> Vec<(String, i32)> {
        let mut results = Vec::new();
        if paths.is_empty() {
            return results;
        }

        let normalized: Vec<String> = paths
            .iter()
            .map(|p| {
                if p.ends_with('/') || p.ends_with('\\') {
                    p.clone()
                } else {
                    format!("{}/", p)
                }
            })
            .collect();

        let tracks = TrackStore::get().get_all();
        for path in normalized {
            let count = tracks
                .iter()
                .filter(|t| t.folder.starts_with(&path))
                .count() as i32;
            results.push((path, count));
        }

        results
    }

    /// Create a folder entry
    fn make_folder(path: &str, track_folders: &[String]) -> Folder {
        let path_buf = std::path::PathBuf::from(path);
        let name = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());

        let has_tracks = track_folders.contains(&path.to_string());
        let track_count = if has_tracks {
            track_folders.iter().filter(|f| *f == path).count() as i32
        } else {
            0
        };

        Folder {
            name,
            path: path.to_string(),
            is_sym: false,
            trackcount: track_count,
        }
    }

    /// Get all folders
    pub fn get_all(&self) -> Vec<Folder> {
        self.folders.read().unwrap().values().cloned().collect()
    }

    /// Get folder by path
    pub fn get_by_path(&self, path: &str) -> Option<Folder> {
        self.folders.read().unwrap().get(path).cloned()
    }

    /// Get children folders
    pub fn get_children(&self, parent_path: &str) -> Vec<Folder> {
        let folders = self.folders.read().unwrap();
        let parent_clean = if parent_path.ends_with('/') || parent_path.ends_with('\\') {
            parent_path.to_string()
        } else {
            format!("{}/", parent_path)
        };

        folders
            .values()
            .filter(|f| {
                f.path != parent_path && f.path.starts_with(&parent_clean) && {
                    // Only direct children (no additional separators)
                    let remainder = &f.path[parent_clean.len()..];
                    !remainder.contains('/') && !remainder.contains('\\')
                }
            })
            .cloned()
            .collect()
    }

    /// Get subfolders in a directory
    pub fn get_subfolders(&self, dir_path: &str) -> Vec<Folder> {
        self.get_children(dir_path)
    }

    /// Check if folder exists
    pub fn exists(&self, path: &str) -> bool {
        self.folders.read().unwrap().contains_key(path)
    }

    /// Check if path is root
    pub fn is_root(&self, path: &str) -> bool {
        self.root_dirs.read().unwrap().contains(&path.to_string())
    }

    /// Clear the store
    pub fn clear(&self) {
        self.folders.write().unwrap().clear();
    }
}
