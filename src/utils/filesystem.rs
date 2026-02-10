//! Filesystem utilities

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Supported audio file extensions
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "wav", "m4a", "ogg", "wma", "opus", "alac", "aiff",
    "ape", "wv", "mpc", "tta", "dsf", "dff", "webm", "mka", "spx",
];

/// Paths to skip during scanning
pub const SKIP_PATHS: &[&str] = &[
    "node_modules",
    "site-packages",
    "__pycache__",
    ".git",
    ".svn",
    ".hg",
    "venv",
    ".venv",
    "env",
    ".env",
];

/// Paths containing these strings are skipped
pub const SKIP_CONTAINS: &[&str] = &["AppData", "Application Support"];

/// Check if a file has a supported audio extension
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Check if a path should be skipped
pub fn should_skip_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Skip hidden files and directories
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || name_str.starts_with('$') {
            return true;
        }
    }

    // Check skip paths
    for skip in SKIP_PATHS {
        if path_str.contains(skip) {
            return true;
        }
    }

    // Check skip contains
    for skip in SKIP_CONTAINS {
        if path_str.contains(skip) {
            return true;
        }
    }

    // Skip macOS Library folder
    #[cfg(target_os = "macos")]
    if path_str.contains("/Library/") {
        return true;
    }

    false
}

/// Scan a directory for audio files
pub fn scan_for_audio_files(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut folders = Vec::new();

    let walker = WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| !should_skip_path(e.path()));

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_file() && is_audio_file(path) {
            files.push(path.to_path_buf());
        } else if path.is_dir() && path != root {
            folders.push(path.to_path_buf());
        }
    }

    (folders, files)
}

/// Get files and directories in a folder (non-recursive)
pub fn get_files_and_dirs(folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            if should_skip_path(&path) {
                continue;
            }

            if path.is_file() && is_audio_file(&path) {
                files.push(path);
            } else if path.is_dir() {
                dirs.push(path);
            }
        }
    }

    // Sort by name
    files.sort();
    dirs.sort();

    (dirs, files)
}

/// Get all file extensions in a directory
pub fn get_extensions_in_dir(dir: &Path) -> HashSet<String> {
    let mut extensions = HashSet::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            if let Some(ext) = entry.path().extension() {
                if let Some(ext_str) = ext.to_str() {
                    extensions.insert(ext_str.to_lowercase());
                }
            }
        }
    }

    extensions
}

/// Normalize path separators for cross-platform compatibility
pub fn normalize_path(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

/// Get the parent folder name
pub fn get_folder_name(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

/// Check if path is a child of parent
pub fn is_child_of(path: &Path, parent: &Path) -> bool {
    path.starts_with(parent) && path != parent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_audio_file() {
        assert!(is_audio_file(Path::new("test.mp3")));
        assert!(is_audio_file(Path::new("test.FLAC")));
        assert!(!is_audio_file(Path::new("test.txt")));
        assert!(!is_audio_file(Path::new("test")));
    }

    #[test]
    fn test_should_skip_path() {
        assert!(should_skip_path(Path::new(".hidden")));
        assert!(should_skip_path(Path::new("$recycle")));
        assert!(should_skip_path(Path::new("node_modules/package")));
        assert!(!should_skip_path(Path::new("music/album")));
    }

    #[test]
    fn test_normalize_path() {
        let path = "C:\\Users\\Music\\test.mp3";
        let normalized = normalize_path(path);

        #[cfg(windows)]
        assert_eq!(normalized, "C:/Users/Music/test.mp3");

        #[cfg(not(windows))]
        assert_eq!(normalized, path);
    }
}
