//! Album store - in-memory album storage with efficient lookups

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::core::albums::AlbumLib;
use crate::db::tables::TrackTable;
use crate::models::Album;
use anyhow::Result;

/// Global album store instance
static ALBUM_STORE: OnceLock<Arc<AlbumStore>> = OnceLock::new();

/// In-memory store for albums
pub struct AlbumStore {
    /// All albums by albumhash
    albums: RwLock<HashMap<String, Album>>,
    /// Albums by artist hash
    albums_by_artist: RwLock<HashMap<String, Vec<String>>>,
}

impl AlbumStore {
    /// Get or initialize the global album store
    pub fn get() -> Arc<AlbumStore> {
        ALBUM_STORE
            .get_or_init(|| {
                Arc::new(AlbumStore {
                    albums: RwLock::new(HashMap::new()),
                    albums_by_artist: RwLock::new(HashMap::new()),
                })
            })
            .clone()
    }

    /// Load albums from database into memory
    pub fn load(&self, albums: Vec<Album>) {
        let mut album_map = self.albums.write().unwrap();
        let mut artist_map = self.albums_by_artist.write().unwrap();

        album_map.clear();
        artist_map.clear();

        for album in albums {
            let hash = album.albumhash.clone();

            // Index by artists
            for artist in &album.artisthashes {
                artist_map
                    .entry(artist.clone())
                    .or_insert_with(Vec::new)
                    .push(hash.clone());
            }

            album_map.insert(hash, album);
        }
    }

    /// Get total album count
    pub fn count(&self) -> usize {
        self.albums.read().unwrap().len()
    }

    /// Get all albums
    pub fn get_all(&self) -> Vec<Album> {
        self.albums.read().unwrap().values().cloned().collect()
    }

    /// Get all album hashes
    pub fn get_all_hashes(&self) -> Vec<String> {
        self.albums.read().unwrap().keys().cloned().collect()
    }

    /// Get album by hash
    pub fn get_by_hash(&self, hash: &str) -> Option<Album> {
        self.albums.read().unwrap().get(hash).cloned()
    }

    /// increment play metrics for an album in place
    pub fn increment_play_stats(&self, albumhash: &str, duration: i32, timestamp: i64) {
        if let Some(album) = self.albums.write().unwrap().get_mut(albumhash) {
            album.playcount += 1;
            album.playduration += duration;
            album.lastplayed = timestamp;
        }
    }

    /// Get albums by hashes
    pub fn get_by_hashes(&self, hashes: &[String]) -> Vec<Album> {
        let albums = self.albums.read().unwrap();
        hashes
            .iter()
            .filter_map(|h| albums.get(h).cloned())
            .collect()
    }

    /// Get albums by artist hash
    pub fn get_by_artist(&self, artist_hash: &str) -> Vec<Album> {
        let artist_map = self.albums_by_artist.read().unwrap();
        if let Some(hashes) = artist_map.get(artist_hash) {
            self.get_by_hashes(hashes)
        } else {
            Vec::new()
        }
    }

    /// Check if album exists
    pub fn exists(&self, hash: &str) -> bool {
        self.albums.read().unwrap().contains_key(hash)
    }

    /// Add an album to the store
    pub fn add(&self, album: Album) {
        let hash = album.albumhash.clone();

        // Add to artist indices
        for artist in &album.artisthashes {
            self.albums_by_artist
                .write()
                .unwrap()
                .entry(artist.clone())
                .or_insert_with(Vec::new)
                .push(hash.clone());
        }

        // Add to main map
        self.albums.write().unwrap().insert(hash, album);
    }

    /// Mark or unmark album as favorite (no user scoping)
    pub fn mark_favorite(&self, albumhash: &str, favorite: bool) {
        if let Some(mut album) = self.get_by_hash(albumhash) {
            if favorite {
                album.fav_userids.insert(0);
            } else {
                album.fav_userids.remove(&0);
            }
            self.add(album);
        }
    }

    /// Set dominant color for an album
    pub fn set_color(&self, albumhash: &str, color: &str) {
        if let Some(mut album) = self.get_by_hash(albumhash) {
            album.color = color.to_string();
            self.add(album);
        }
    }

    /// Load albums by deriving from track table
    pub async fn load_albums() -> Result<()> {
        let tracks = TrackTable::all().await?;
        let albums = AlbumLib::build_albums(&tracks);
        AlbumStore::get().load(albums);
        Ok(())
    }

    /// Update an album in the store
    pub fn update(&self, album: Album) {
        let hash = album.albumhash.clone();

        // Remove from old artist indices if exists
        if let Some(old_album) = self.get_by_hash(&hash) {
            let mut artist_map = self.albums_by_artist.write().unwrap();
            for artist in &old_album.artisthashes {
                if let Some(artist_albums) = artist_map.get_mut(artist) {
                    artist_albums.retain(|h| h != &hash);
                }
            }
        }

        // Add to new artist indices
        {
            let mut artist_map = self.albums_by_artist.write().unwrap();
            for artist in &album.artisthashes {
                artist_map
                    .entry(artist.clone())
                    .or_insert_with(Vec::new)
                    .push(hash.clone());
            }
        }

        // Update in main map
        self.albums.write().unwrap().insert(hash, album);
    }

    /// Remove an album from the store
    pub fn remove(&self, hash: &str) {
        if let Some(album) = self.albums.write().unwrap().remove(hash) {
            let mut artist_map = self.albums_by_artist.write().unwrap();
            for artist in &album.artisthashes {
                if let Some(artist_albums) = artist_map.get_mut(artist) {
                    artist_albums.retain(|h| h != hash);
                }
            }
        }
    }

    /// Remove albums with no tracks
    pub fn remove_empty(&self, track_album_hashes: &[String]) {
        let track_hashes: std::collections::HashSet<_> = track_album_hashes.iter().collect();
        let hashes_to_remove: Vec<_> = self
            .albums
            .read()
            .unwrap()
            .keys()
            .filter(|h| !track_hashes.contains(h))
            .cloned()
            .collect();

        for hash in hashes_to_remove {
            self.remove(&hash);
        }
    }

    /// Clear the store
    pub fn clear(&self) {
        self.albums.write().unwrap().clear();
        self.albums_by_artist.write().unwrap().clear();
    }
}
