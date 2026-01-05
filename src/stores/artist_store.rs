//! Artist store - in-memory artist storage with efficient lookups

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::core::artistlib::ArtistLib;
use crate::models::Artist;
use crate::stores::TrackStore;
use anyhow::Result;

/// Global artist store instance
static ARTIST_STORE: OnceLock<Arc<ArtistStore>> = OnceLock::new();

/// In-memory store for artists
pub struct ArtistStore {
    /// All artists by artisthash
    artists: RwLock<HashMap<String, Artist>>,
    /// Artists by name (lowercase for searching)
    artists_by_name: RwLock<HashMap<String, String>>,
}

impl ArtistStore {
    /// Get or initialize the global artist store
    pub fn get() -> Arc<ArtistStore> {
        ARTIST_STORE
            .get_or_init(|| {
                Arc::new(ArtistStore {
                    artists: RwLock::new(HashMap::new()),
                    artists_by_name: RwLock::new(HashMap::new()),
                })
            })
            .clone()
    }

    /// Load artists from database into memory
    pub fn load(&self, artists: Vec<Artist>) {
        let mut artist_map = self.artists.write().unwrap();
        let mut name_map = self.artists_by_name.write().unwrap();

        artist_map.clear();
        name_map.clear();

        for artist in artists {
            let mut artist = artist;

            // generate artist image path if not already set
            if artist.image.is_empty() {
                artist.set_image();
            }

            let hash = artist.artisthash.clone();
            let name = artist.name.to_lowercase();

            name_map.insert(name, hash.clone());
            artist_map.insert(hash, artist);
        }
    }

    /// Get total artist count
    pub fn count(&self) -> usize {
        self.artists.read().unwrap().len()
    }

    /// Get all artists
    pub fn get_all(&self) -> Vec<Artist> {
        self.artists.read().unwrap().values().cloned().collect()
    }

    /// Get all artist hashes
    pub fn get_all_hashes(&self) -> Vec<String> {
        self.artists.read().unwrap().keys().cloned().collect()
    }

    /// Get artist by hash
    pub fn get_by_hash(&self, hash: &str) -> Option<Artist> {
        self.artists.read().unwrap().get(hash).cloned()
    }

    /// increment play metrics for an artist in place
    pub fn increment_play_stats(&self, artisthash: &str, duration: i32, timestamp: i64) {
        if let Some(artist) = self.artists.write().unwrap().get_mut(artisthash) {
            artist.playcount += 1;
            artist.playduration += duration;
            artist.lastplayed = timestamp;
        }
    }

    /// Get artists by hashes
    pub fn get_by_hashes(&self, hashes: &[String]) -> Vec<Artist> {
        let artists = self.artists.read().unwrap();
        hashes
            .iter()
            .filter_map(|h| artists.get(h).cloned())
            .collect()
    }

    /// Get artist by name
    pub fn get_by_name(&self, name: &str) -> Option<Artist> {
        let name_lower = name.to_lowercase();
        let name_map = self.artists_by_name.read().unwrap();
        if let Some(hash) = name_map.get(&name_lower) {
            self.get_by_hash(hash)
        } else {
            None
        }
    }

    /// Check if artist exists
    pub fn exists(&self, hash: &str) -> bool {
        self.artists.read().unwrap().contains_key(hash)
    }

    /// Add an artist to the store
    pub fn add(&self, artist: Artist) {
        let mut artist = artist;

        // generate artist image path if not already set
        if artist.image.is_empty() {
            artist.set_image();
        }

        let hash = artist.artisthash.clone();
        let name = artist.name.to_lowercase();

        self.artists_by_name
            .write()
            .unwrap()
            .insert(name, hash.clone());
        self.artists.write().unwrap().insert(hash, artist);
    }

    /// Update an artist in the store
    pub fn update(&self, artist: Artist) {
        let hash = artist.artisthash.clone();

        // Remove old name index if exists
        if let Some(old_artist) = self.get_by_hash(&hash) {
            let old_name = old_artist.name.to_lowercase();
            self.artists_by_name.write().unwrap().remove(&old_name);
        }

        // Add new name index
        let name = artist.name.to_lowercase();
        self.artists_by_name
            .write()
            .unwrap()
            .insert(name, hash.clone());

        // Update main map
        self.artists.write().unwrap().insert(hash, artist);
    }

    /// Remove an artist from the store
    pub fn remove(&self, hash: &str) {
        if let Some(artist) = self.artists.write().unwrap().remove(hash) {
            let name = artist.name.to_lowercase();
            self.artists_by_name.write().unwrap().remove(&name);
        }
    }

    /// Remove artists with no tracks
    pub fn remove_orphaned(&self, valid_hashes: &[String]) {
        let valid_set: std::collections::HashSet<_> = valid_hashes.iter().collect();
        let hashes_to_remove: Vec<_> = self
            .artists
            .read()
            .unwrap()
            .keys()
            .filter(|h| !valid_set.contains(h))
            .cloned()
            .collect();

        for hash in hashes_to_remove {
            self.remove(&hash);
        }
    }

    /// Mark or unmark an artist as favorite (no user scoping)
    pub fn mark_favorite(&self, artisthash: &str, favorite: bool) {
        if let Some(mut artist) = self.get_by_hash(artisthash) {
            if favorite {
                artist.fav_userids.insert(0);
            } else {
                artist.fav_userids.remove(&0);
            }
            self.add(artist);
        }
    }

    /// Set image for an artist
    pub fn set_image(&self, artisthash: &str, image: &str) {
        if let Some(mut artist) = self.get_by_hash(artisthash) {
            artist.image = image.to_string();
            self.add(artist);
        }
    }

    /// Set color for an artist
    pub fn set_color(&self, artisthash: &str, color: &str) {
        if let Some(mut artist) = self.get_by_hash(artisthash) {
            artist.color = color.to_string();
            self.add(artist);
        }
    }

    /// Load artists derived from tracks into memory
    pub async fn load_artists() -> Result<()> {
        let tracks = TrackStore::get().get_all();
        let artists = ArtistLib::build_artists(&tracks);
        ArtistStore::get().load(artists);
        Ok(())
    }

    /// Clear the store
    pub fn clear(&self) {
        self.artists.write().unwrap().clear();
        self.artists_by_name.write().unwrap().clear();
    }

    /// Search artists by name (case-insensitive prefix match)
    pub fn search_by_name(&self, query: &str, limit: usize) -> Vec<Artist> {
        let query_lower = query.to_lowercase();
        let artists = self.artists.read().unwrap();

        artists
            .values()
            .filter(|a| a.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .cloned()
            .collect()
    }
}
