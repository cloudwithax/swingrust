//! Music library indexer - scans directories and extracts metadata using lofty
//!
//! this module provides high-performance parallel indexing of audio files using:
//! - lofty for in-process metadata extraction (no subprocess spawning)
//! - rayon for parallel file processing across all cpu cores
//! - pre-cached config to avoid repeated disk i/o

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use lofty::{Accessor, AudioFile, ItemKey, Probe, TaggedFileExt};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use walkdir::{DirEntry, WalkDir};

use crate::config::UserConfig;
use crate::models::Track;
use crate::utils::hashing::{create_hash, create_track_hash};
use crate::utils::artist_split_detector::split_artists_smart;
use crate::utils::parsers::clean_title;
use crate::utils::tracks::remove_remaster_info;

/// supported audio extensions
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "wav", "m4a", "aac", "wma", "opus", "aiff", "alac",
];

/// pre-cached config data needed for track extraction
/// avoids loading config from disk for every single file
#[derive(Clone)]
struct IndexerConfig {
    artist_separators: HashSet<String>,
    artist_split_ignore_list: HashSet<String>,
    genre_separators: HashSet<String>,
}

impl IndexerConfig {
    fn from_user_config(config: &UserConfig) -> Self {
        Self {
            artist_separators: config.artist_separators.clone(),
            artist_split_ignore_list: config.artist_split_ignore_list.clone(),
            genre_separators: config.genre_separators.clone(),
        }
    }
}

/// music library indexer with parallel processing
pub struct Indexer {
    root_dirs: Vec<PathBuf>,
    artist_separators: Vec<String>,
    show_progress: bool,
}

impl Indexer {
    /// create new indexer with root directories
    pub fn new(root_dirs: Vec<String>, artist_separators: Vec<String>) -> Self {
        Self {
            root_dirs: root_dirs.into_iter().map(PathBuf::from).collect(),
            artist_separators,
            show_progress: true,
        }
    }

    /// create indexer from user config
    pub fn from_config(config: &UserConfig) -> Self {
        Self::new(
            config.root_dirs.clone(),
            config.artist_separators.iter().cloned().collect(),
        )
    }

    /// set whether to show progress bar
    pub fn with_progress(mut self, show: bool) -> Self {
        self.show_progress = show;
        self
    }

    /// check if file is an audio file
    fn is_audio_file(entry: &DirEntry) -> bool {
        if !entry.file_type().is_file() {
            return false;
        }

        entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// check if directory should be skipped
    fn should_skip_dir(entry: &DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    }

    /// scan directories and return list of audio file paths
    pub fn scan_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        for root in &self.root_dirs {
            if !root.exists() {
                tracing::warn!("root directory does not exist: {}", root.display());
                continue;
            }

            let walker = WalkDir::new(root)
                .follow_links(true)
                .into_iter()
                .filter_entry(|e| !Self::should_skip_dir(e));

            for entry in walker.filter_map(|e| e.ok()) {
                if Self::is_audio_file(&entry) {
                    files.push(entry.path().to_path_buf());
                }
            }
        }

        files
    }

    /// scan and extract tracks from all directories using parallel processing
    pub fn index(&self) -> Result<Vec<Track>> {
        let files = self.scan_files();
        let total_files = files.len();

        if total_files == 0 {
            return Ok(Vec::new());
        }

        // pre-load config once for all files
        let user_config = UserConfig::load()?;
        let indexer_config = Arc::new(IndexerConfig::from_user_config(&user_config));

        let progress = if self.show_progress {
            let pb = ProgressBar::new(total_files as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
                    .unwrap()
                    .progress_chars("##-"),
            );
            Some(Arc::new(pb))
        } else {
            None
        };

        // atomic counter for progress updates
        let processed = Arc::new(AtomicU64::new(0));

        // process files in parallel using rayon
        let tracks: Vec<Track> = files
            .par_iter()
            .filter_map(|path| {
                let result = extract_track_lofty(path, &indexer_config);

                // update progress
                let count = processed.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(pb) = &progress {
                    pb.set_position(count);
                    if count % 100 == 0 || count == total_files as u64 {
                        pb.set_message(format!("{} files", count));
                    }
                }

                match result {
                    Ok(track) => Some(track),
                    Err(e) => {
                        tracing::debug!("failed to read metadata from {}: {}", path.display(), e);
                        None
                    }
                }
            })
            .collect();

        if let Some(pb) = progress {
            pb.finish_with_message(format!("indexed {} tracks", tracks.len()));
        }

        Ok(tracks)
    }

    /// re-index specific files using parallel processing
    pub fn reindex_files(&self, paths: &[PathBuf]) -> Result<Vec<Track>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // pre-load config once
        let user_config = UserConfig::load()?;
        let indexer_config = Arc::new(IndexerConfig::from_user_config(&user_config));

        let tracks: Vec<Track> = paths
            .par_iter()
            .filter(|path| path.exists())
            .filter_map(|path| {
                match extract_track_lofty(path, &indexer_config) {
                    Ok(track) => Some(track),
                    Err(e) => {
                        tracing::warn!("failed to reindex {}: {}", path.display(), e);
                        None
                    }
                }
            })
            .collect();

        Ok(tracks)
    }
}

/// extract track metadata from a file using lofty (pure rust, no subprocess)
fn extract_track_lofty(path: &Path, config: &IndexerConfig) -> Result<Track> {
    // read the audio file with lofty
    let tagged_file = Probe::open(path)
        .map_err(|e| anyhow::anyhow!("failed to open file: {}", e))?
        .read()
        .map_err(|e| anyhow::anyhow!("failed to read tags: {}", e))?;

    let filepath = path.to_string_lossy().to_string();
    let folder = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // get the primary tag or first available tag
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    // extract basic metadata
    let title = tag
        .and_then(|t| t.title().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

    let album = tag
        .and_then(|t| t.album().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Album".to_string());

    let artist = tag
        .and_then(|t| t.artist().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let album_artist = tag
        .and_then(|t| t.get_string(&ItemKey::AlbumArtist).map(|s| s.to_string()))
        .unwrap_or_else(|| artist.clone());

    let genre = tag.and_then(|t| t.genre().map(|s| s.to_string()));
    
    let copyright = tag.and_then(|t| {
        t.get_string(&ItemKey::CopyrightMessage)
            .or_else(|| t.get_string(&ItemKey::Unknown("COPYRIGHT".to_string())))
            .map(|s| s.to_string())
    });

    let track_number = tag.and_then(|t| t.track()).map(|n| n as i32);
    let disc_number = tag.and_then(|t| t.disk()).map(|n| n as i32);

    // extract year from tag - need to handle full date strings like "2025-01-15"
    // lofty's year() method doesn't properly parse full ISO dates from TDRC/DATE tags
    let year: Option<i32> = tag.and_then(|t| {
        // try to get the raw date string from various date-related item keys
        let date_keys = [
            ItemKey::RecordingDate,
            ItemKey::OriginalReleaseDate,
            ItemKey::Year,
        ];

        for key in date_keys {
            if let Some(date_str) = t.get_string(&key) {
                let s = date_str.trim();
                // extract leading 4 digits as year (handles "2025", "2025-01-15", "2025-01-15T12:34:00", etc.)
                if s.len() >= 4 && s[..4].chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(y) = s[..4].parse::<i32>() {
                        return Some(y);
                    }
                }
            }
        }

        // fallback to the convenience year() method
        t.year().map(|y| y as i32)
    });

    // get audio properties for duration and bitrate
    let properties = tagged_file.properties();
    let duration = properties.duration().as_secs() as i32;
    let bitrate = properties.audio_bitrate().unwrap_or(0) as i32;

    // get file modification time
    let last_mod = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
        .unwrap_or(0);

    // clean title
    let clean = clean_title(&title);
    let cleaned_title = remove_remaster_info(&clean);

    // split artists using pre-cached config
    let mut artist_names: Vec<String> = tag
        .map(|t| t.get_strings(&ItemKey::TrackArtist).map(|s| s.to_string()).collect())
        .unwrap_or_default();

    if artist_names.is_empty() {
        artist_names = split_artists_smart(&artist, &config.artist_separators, &config.artist_split_ignore_list);
    } else if artist_names.len() == 1 {
        // if single value, it might still need splitting (e.g. joined by separators)
        artist_names = split_artists_smart(&artist_names[0], &config.artist_separators, &config.artist_split_ignore_list);
    }

    let mut album_artist_names: Vec<String> = tag
        .map(|t| t.get_strings(&ItemKey::AlbumArtist).map(|s| s.to_string()).collect())
        .unwrap_or_default();

    if album_artist_names.is_empty() {
        // fallback to splitting the album_artist string we resolved earlier
        album_artist_names = split_artists_smart(&album_artist, &config.artist_separators, &config.artist_split_ignore_list);
    } else if album_artist_names.len() == 1 {
        album_artist_names = split_artists_smart(&album_artist_names[0], &config.artist_separators, &config.artist_split_ignore_list);
    }

    // create artist refs with hashes
    let artists: Vec<crate::models::ArtistRefItem> = artist_names
        .iter()
        .map(|name| {
            let artisthash = create_hash(&[name], true);
            crate::models::ArtistRefItem::new(name.clone(), artisthash)
        })
        .collect();

    let albumartists: Vec<crate::models::ArtistRefItem> = album_artist_names
        .iter()
        .map(|name| {
            let artisthash = create_hash(&[name], true);
            crate::models::ArtistRefItem::new(name.clone(), artisthash)
        })
        .collect();

    // extract artist and genre hashes
    let artisthashes: Vec<String> = artists.iter().map(|a| a.artisthash.clone()).collect();

    // parse genres using pre-cached separators
    let mut genre_names: Vec<String> = tag
        .map(|t| t.get_strings(&ItemKey::Genre).map(|s| s.to_string()).collect())
        .unwrap_or_default();

    if genre_names.is_empty() {
        if let Some(g) = &genre {
            genre_names = config
                .genre_separators
                .iter()
                .fold(vec![g.as_str()], |acc, sep| {
                    acc.into_iter().flat_map(|s| s.split(sep)).collect()
                })
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    } else if genre_names.len() == 1 {
        genre_names = config
            .genre_separators
            .iter()
            .fold(vec![genre_names[0].as_str()], |acc, sep| {
                acc.into_iter().flat_map(|s| s.split(sep)).collect()
            })
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    let genres: Vec<crate::models::GenreRef> = genre_names
        .iter()
        .map(|name| {
            let genrehash = create_hash(&[name], true);
            crate::models::GenreRef::new(name.clone(), genrehash)
        })
        .collect();

    let genrehashes: Vec<String> = genres.iter().map(|g| g.genrehash.clone()).collect();

    // create hashes
    let og_title = cleaned_title.clone();
    let og_album = album.clone();
    let albumhash = create_hash(&[&og_album, &album_artist_names.join("-")], true);
    let trackhash = create_track_hash(&artist_names.join(", "), &og_album, &og_title);
    let weakhash = create_hash(&[&og_album, &og_title], true);

    // parse date to timestamp
    let date_timestamp = if let Some(y) = year {
        chrono::NaiveDate::from_ymd_opt(y, 1, 1)
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            .unwrap_or(0)
    } else {
        0
    };

    Ok(Track {
        id: 0, // will be set by database
        trackhash,
        title: cleaned_title,
        album,
        og_album,
        og_title,
        albumhash,
        artists,
        albumartists,
        artisthashes,
        filepath,
        folder,
        duration,
        bitrate,
        track: track_number.unwrap_or(0),
        disc: disc_number.unwrap_or(1),
        date: date_timestamp,
        genres,
        genrehashes,
        last_mod,
        image: String::new(),
        copyright,
        extra: serde_json::Value::Null,
        lastplayed: 0,
        playcount: 0,
        playduration: 0,
        weakhash,
        pos: None,
        help_text: String::new(),
        score: 0.0,
        explicit: false,
        fav_userids: HashSet::new(),
    })
}
