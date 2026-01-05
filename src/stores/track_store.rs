//! Track store - in-memory track storage with efficient lookups

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::db::tables::TrackTable;
use crate::utils::filesystem::normalize_path;
use anyhow::Result;

use crate::models::Track;

/// Global track store instance
static TRACK_STORE: OnceLock<Arc<TrackStore>> = OnceLock::new();

/// In-memory store for tracks
pub struct TrackStore {
    /// All tracks by trackhash
    tracks: RwLock<HashMap<String, Track>>,
    /// Tracks by filepath
    tracks_by_path: RwLock<HashMap<String, String>>,
    /// Tracks by album hash
    tracks_by_album: RwLock<HashMap<String, Vec<String>>>,
    /// Tracks by artist hash
    tracks_by_artist: RwLock<HashMap<String, Vec<String>>>,
    /// Tracks by folder path
    tracks_by_folder: RwLock<HashMap<String, Vec<String>>>,
}

impl TrackStore {
    /// Get or initialize the global track store
    pub fn get() -> Arc<TrackStore> {
        TRACK_STORE
            .get_or_init(|| {
                Arc::new(TrackStore {
                    tracks: RwLock::new(HashMap::new()),
                    tracks_by_path: RwLock::new(HashMap::new()),
                    tracks_by_album: RwLock::new(HashMap::new()),
                    tracks_by_artist: RwLock::new(HashMap::new()),
                    tracks_by_folder: RwLock::new(HashMap::new()),
                })
            })
            .clone()
    }

    /// Load tracks from database into memory
    pub fn load(&self, tracks: Vec<Track>) {
        let mut track_map = self.tracks.write().unwrap();
        let mut path_map = self.tracks_by_path.write().unwrap();
        let mut album_map = self.tracks_by_album.write().unwrap();
        let mut artist_map = self.tracks_by_artist.write().unwrap();
        let mut folder_map = self.tracks_by_folder.write().unwrap();

        track_map.clear();
        path_map.clear();
        album_map.clear();
        artist_map.clear();
        folder_map.clear();
        for track in tracks {
            let mut track = track;

            // normalize paths so lookups remain consistent across os path separators
            track.filepath = normalize_path(&track.filepath);
            track.folder = normalize_path(&track.folder);

            // generate album art image path if not already set
            if track.image.is_empty() {
                track.generate_image();
            }

            let hash = track.trackhash.clone();
            let path = track.filepath.clone();
            let album = track.albumhash.clone();
            let folder = track.folder.clone();

            // Index by path
            path_map.insert(path, hash.clone());

            // Index by album
            album_map
                .entry(album)
                .or_insert_with(Vec::new)
                .push(hash.clone());

            // index by all artists associated with this track (both track artists and album artists)
            // use a set to avoid duplicate entries when an artist appears in both roles
            let mut all_artist_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
            
            // include track artists from the artists list
            for artist_ref in &track.artists {
                all_artist_hashes.insert(artist_ref.artisthash.clone());
            }
            
            // include album artists
            for album_artist in &track.albumartists {
                all_artist_hashes.insert(album_artist.artisthash.clone());
            }
            
            // also include any hashes from the precomputed artisthashes field (in case of discrepancies)
            for artist_hash in &track.artisthashes {
                all_artist_hashes.insert(artist_hash.clone());
            }
            
            // index track under all associated artists
            for artist_hash in all_artist_hashes {
                artist_map
                    .entry(artist_hash)
                    .or_insert_with(Vec::new)
                    .push(hash.clone());
            }

            // Index by folder
            folder_map
                .entry(folder)
                .or_insert_with(Vec::new)
                .push(hash.clone());

            track_map.insert(hash, track);
        }
    }

    /// Get total track count
    pub fn count(&self) -> usize {
        self.tracks.read().unwrap().len()
    }

    /// Get all tracks
    pub fn get_all(&self) -> Vec<Track> {
        self.tracks.read().unwrap().values().cloned().collect()
    }

    /// Get all track hashes
    pub fn get_all_hashes(&self) -> Vec<String> {
        self.tracks.read().unwrap().keys().cloned().collect()
    }

    /// Get track by hash
    pub fn get_by_hash(&self, hash: &str) -> Option<Track> {
        self.tracks.read().unwrap().get(hash).cloned()
    }

    /// Get only the filepath for a track by hash (avoids cloning full Track)
    pub fn get_filepath_by_hash(&self, hash: &str) -> Option<String> {
        self.tracks
            .read()
            .unwrap()
            .get(hash)
            .map(|t| t.filepath.clone())
    }

    /// Check if a track exists by hash (no cloning)
    pub fn exists(&self, hash: &str) -> bool {
        self.tracks.read().unwrap().contains_key(hash)
    }

    /// increment play metrics for a track in place
    pub fn increment_play_stats(&self, trackhash: &str, duration: i32, timestamp: i64) {
        if let Some(track) = self.tracks.write().unwrap().get_mut(trackhash) {
            track.playcount += 1;
            track.playduration += duration;
            track.lastplayed = timestamp;
        }
    }

    /// Get tracks by hashes
    pub fn get_by_hashes(&self, hashes: &[String]) -> Vec<Track> {
        let tracks = self.tracks.read().unwrap();
        hashes
            .iter()
            .filter_map(|h| tracks.get(h).cloned())
            .collect()
    }

    /// Get track by filepath
    pub fn get_by_path(&self, path: &str) -> Option<Track> {
        let path_map = self.tracks_by_path.read().unwrap();
        let normalized = normalize_path(path);

        if let Some(hash) = path_map.get(path).or_else(|| path_map.get(&normalized)) {
            self.get_by_hash(hash)
        } else {
            None
        }
    }

    /// Get tracks by album hash
    pub fn get_by_album(&self, album_hash: &str) -> Vec<Track> {
        let album_map = self.tracks_by_album.read().unwrap();
        if let Some(hashes) = album_map.get(album_hash) {
            self.get_by_hashes(hashes)
        } else {
            Vec::new()
        }
    }

    /// Get tracks by artist hash
    pub fn get_by_artist(&self, artist_hash: &str) -> Vec<Track> {
        let artist_map = self.tracks_by_artist.read().unwrap();
        if let Some(hashes) = artist_map.get(artist_hash) {
            self.get_by_hashes(hashes)
        } else {
            Vec::new()
        }
    }

    /// Get tracks by folder path
    pub fn get_by_folder(&self, folder: &str) -> Vec<Track> {
        let folder_map = self.tracks_by_folder.read().unwrap();
        let normalized = normalize_path(folder);

        if let Some(hashes) = folder_map
            .get(folder)
            .or_else(|| folder_map.get(&normalized))
        {
            self.get_by_hashes(hashes)
        } else {
            Vec::new()
        }
    }

    /// Check if path exists
    pub fn path_exists(&self, path: &str) -> bool {
        let normalized = normalize_path(path);
        let map = self.tracks_by_path.read().unwrap();
        map.contains_key(path) || map.contains_key(&normalized)
    }

    /// Get all filepaths
    pub fn get_all_paths(&self) -> Vec<String> {
        self.tracks_by_path
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Add a track to the store
    pub fn add(&self, mut track: Track) {
        // normalize paths to match filesystem queries regardless of separator style
        track.filepath = normalize_path(&track.filepath);
        track.folder = normalize_path(&track.folder);

        // generate album art image path if not already set
        if track.image.is_empty() {
            track.generate_image();
        }

        let hash = track.trackhash.clone();
        let path = track.filepath.clone();
        let album = track.albumhash.clone();
        let folder = track.folder.clone();

        // Add to path index
        self.tracks_by_path
            .write()
            .unwrap()
            .insert(path, hash.clone());

        // Add to album index
        self.tracks_by_album
            .write()
            .unwrap()
            .entry(album)
            .or_insert_with(Vec::new)
            .push(hash.clone());

        // add to artist indices (all artists associated with this track)
        let mut all_artist_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
        for artist_ref in &track.artists {
            all_artist_hashes.insert(artist_ref.artisthash.clone());
        }
        for album_artist in &track.albumartists {
            all_artist_hashes.insert(album_artist.artisthash.clone());
        }
        for artist_hash in &track.artisthashes {
            all_artist_hashes.insert(artist_hash.clone());
        }
        for artist_hash in all_artist_hashes {
            self.tracks_by_artist
                .write()
                .unwrap()
                .entry(artist_hash)
                .or_insert_with(Vec::new)
                .push(hash.clone());
        }

        // Add to folder index
        self.tracks_by_folder
            .write()
            .unwrap()
            .entry(folder)
            .or_insert_with(Vec::new)
            .push(hash.clone());

        // Add to main map
        self.tracks.write().unwrap().insert(hash, track);
    }

    /// Remove a track by hash and update indices
    pub fn remove(&self, trackhash: &str) -> bool {
        let mut tracks = self.tracks.write().unwrap();
        if let Some(track) = tracks.remove(trackhash) {
            // remove path index
            self.tracks_by_path
                .write()
                .unwrap()
                .retain(|_, h| h != trackhash);
            // remove album index
            if let Some(album_tracks) = self
                .tracks_by_album
                .write()
                .unwrap()
                .get_mut(&track.albumhash)
            {
                album_tracks.retain(|h| h != trackhash);
            }
            // remove from all artist indices
            {
                let mut artist_map = self.tracks_by_artist.write().unwrap();
                // remove from track artists
                for artist_ref in &track.artists {
                    if let Some(vec) = artist_map.get_mut(&artist_ref.artisthash) {
                        vec.retain(|h| h != trackhash);
                    }
                }
                // remove from album artists
                for album_artist in &track.albumartists {
                    if let Some(vec) = artist_map.get_mut(&album_artist.artisthash) {
                        vec.retain(|h| h != trackhash);
                    }
                }
                // also check artisthashes field
                for artist_hash in &track.artisthashes {
                    if let Some(vec) = artist_map.get_mut(artist_hash) {
                        vec.retain(|h| h != trackhash);
                    }
                }
            }
            // remove folder index
            if let Some(folder_tracks) = self
                .tracks_by_folder
                .write()
                .unwrap()
                .get_mut(&track.folder)
            {
                folder_tracks.retain(|h| h != trackhash);
            }
            true
        } else {
            false
        }
    }

    /// Mark or unmark favorite (no user scoping; toggles flag list)
    pub fn mark_favorite(&self, trackhash: &str, favorite: bool) {
        if let Some(mut track) = self.get_by_hash(trackhash) {
            if favorite {
                track.fav_userids.insert(0);
            } else {
                track.fav_userids.remove(&0);
            }
            self.add(track);
        }
    }

    /// Set play count and optionally last played timestamp
    pub fn set_play_count(&self, trackhash: &str, playcount: i32) {
        if let Some(mut track) = self.get_by_hash(trackhash) {
            track.playcount = playcount;
            self.add(track);
        }
    }

    /// Load all tracks from the database into the in-memory store
    pub async fn load_all_tracks() -> Result<()> {
        let tracks = TrackTable::all().await?;
        TrackStore::get().load(tracks);
        Ok(())
    }

    /// Remove tracks by paths
    pub fn remove_by_paths(&self, paths: &[String]) {
        let mut tracks = self.tracks.write().unwrap();
        let mut path_map = self.tracks_by_path.write().unwrap();
        let mut album_map = self.tracks_by_album.write().unwrap();
        let mut artist_map = self.tracks_by_artist.write().unwrap();
        let mut folder_map = self.tracks_by_folder.write().unwrap();

        for path in paths {
            let normalized = normalize_path(path);
            let hash_opt = path_map
                .remove(path)
                .or_else(|| path_map.remove(&normalized));

            if let Some(hash) = hash_opt {
                if let Some(track) = tracks.remove(&hash) {
                    // Remove from album index
                    if let Some(album_tracks) = album_map.get_mut(&track.albumhash) {
                        album_tracks.retain(|h| h != &hash);
                    }

                    // remove from all artist indices
                    for artist_ref in &track.artists {
                        if let Some(artist_tracks) = artist_map.get_mut(&artist_ref.artisthash) {
                            artist_tracks.retain(|h| h != &hash);
                        }
                    }
                    for album_artist in &track.albumartists {
                        if let Some(artist_tracks) = artist_map.get_mut(&album_artist.artisthash) {
                            artist_tracks.retain(|h| h != &hash);
                        }
                    }
                    for artist_hash in &track.artisthashes {
                        if let Some(artist_tracks) = artist_map.get_mut(artist_hash) {
                            artist_tracks.retain(|h| h != &hash);
                        }
                    }

                    // Remove from folder index
                    if let Some(folder_tracks) = folder_map.get_mut(&track.folder) {
                        folder_tracks.retain(|h| h != &hash);
                    }
                }
            }
        }
    }

    /// Clear the store
    pub fn clear(&self) {
        self.tracks.write().unwrap().clear();
        self.tracks_by_path.write().unwrap().clear();
        self.tracks_by_album.write().unwrap().clear();
        self.tracks_by_artist.write().unwrap().clear();
        self.tracks_by_folder.write().unwrap().clear();
    }
}
