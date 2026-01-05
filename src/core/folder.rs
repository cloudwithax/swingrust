//! Folder library functions

use std::path::Path;

use crate::models::{Folder, Track};
use crate::stores::{FolderStore, TrackStore};

/// Folder library functions
pub struct FolderLib;

impl FolderLib {
    /// Get all root directories
    pub fn get_root_dirs() -> Vec<String> {
        FolderStore::get().get_root_dirs()
    }

    /// Get folder by path
    pub fn get_by_path(path: &str) -> Option<Folder> {
        FolderStore::get().get_by_path(path)
    }

    /// Get subfolders in directory
    pub fn get_subfolders(path: &str) -> Vec<Folder> {
        FolderStore::get().get_subfolders(path)
    }

    /// Get tracks in folder
    pub fn get_tracks(folder_path: &str) -> Vec<Track> {
        TrackStore::get().get_by_folder(folder_path)
    }

    /// Get folder contents (subfolders and tracks)
    pub fn get_contents(folder_path: &str) -> (Vec<Folder>, Vec<Track>) {
        let subfolders = Self::get_subfolders(folder_path);
        let tracks = Self::get_tracks(folder_path);
        (subfolders, tracks)
    }

    /// Check if path is root directory
    pub fn is_root(path: &str) -> bool {
        FolderStore::get().is_root(path)
    }

    /// Get breadcrumb path for navigation
    pub fn get_breadcrumbs(path: &str) -> Vec<(String, String)> {
        let root_dirs = Self::get_root_dirs();

        // Find which root this path belongs to
        let root = root_dirs.iter().find(|r| path.starts_with(r.as_str()));

        match root {
            Some(root_path) => {
                let mut breadcrumbs = Vec::new();
                let mut current = Path::new(path);
                let root = Path::new(root_path);

                // Build path from root to current
                while current.starts_with(root) {
                    let name = current.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if !name.is_empty() {
                        breadcrumbs
                            .insert(0, (name.to_string(), current.to_string_lossy().to_string()));
                    }

                    match current.parent() {
                        Some(parent) if parent != current => {
                            current = parent;
                        }
                        _ => break,
                    }
                }

                // Add root
                if let Some(root_name) = Path::new(root_path).file_name() {
                    breadcrumbs.insert(
                        0,
                        (root_name.to_string_lossy().to_string(), root_path.clone()),
                    );
                }

                breadcrumbs
            }
            None => Vec::new(),
        }
    }

    /// Get parent folder path
    pub fn get_parent(path: &str) -> Option<String> {
        Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Check if folder exists
    pub fn exists(path: &str) -> bool {
        FolderStore::get().exists(path)
    }

    /// Check if path is within root directories
    pub fn is_valid_path(path: &str) -> bool {
        let root_dirs = Self::get_root_dirs();
        root_dirs.iter().any(|root| path.starts_with(root.as_str()))
    }

    /// Calculate folder track count recursively
    pub fn recursive_track_count(path: &str) -> usize {
        let mut count = Self::get_tracks(path).len();

        for subfolder in Self::get_subfolders(path) {
            count += Self::recursive_track_count(&subfolder.path);
        }

        count
    }
}
