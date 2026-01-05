//! Homepage entries management

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::core::recipes::{Mix, Recipes};
use crate::models::{Album, Artist, Track};
use crate::stores::{AlbumStore, ArtistStore};

static HOMEPAGE_STORE: OnceLock<Arc<HomepageStore>> = OnceLock::new();

/// Homepage entry type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    RecentlyPlayed,
    RecentlyAdded,
    TopStreamed,
    Mix,
    Artists,
    Albums,
    Custom,
}

/// Homepage entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomepageEntry {
    pub id: String,
    pub entry_type: EntryType,
    pub title: String,
    pub description: String,
    pub items: Vec<HomepageItem>,
    pub visible: bool,
    pub order: i32,
}

/// Homepage item (track, album, artist, or mix)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HomepageItem {
    Track {
        trackhash: String,
        title: String,
        artist: String,
        album: String,
        image: String,
    },
    Album {
        albumhash: String,
        title: String,
        albumartist: String,
        image: String,
    },
    Artist {
        artisthash: String,
        name: String,
        image: String,
    },
    Mix {
        id: String,
        name: String,
        description: String,
        image: Option<String>,
        track_count: usize,
    },
}

impl From<Track> for HomepageItem {
    fn from(t: Track) -> Self {
        let artist = t.artist();
        HomepageItem::Track {
            trackhash: t.trackhash,
            title: t.title,
            artist,
            album: t.album,
            image: t.image,
        }
    }
}

impl From<Album> for HomepageItem {
    fn from(a: Album) -> Self {
        let albumartist = a.albumartist();
        HomepageItem::Album {
            albumhash: a.albumhash,
            title: a.title,
            albumartist,
            image: a.image,
        }
    }
}

impl From<Artist> for HomepageItem {
    fn from(a: Artist) -> Self {
        HomepageItem::Artist {
            artisthash: a.artisthash,
            name: a.name,
            image: a.image,
        }
    }
}

impl From<Mix> for HomepageItem {
    fn from(m: Mix) -> Self {
        HomepageItem::Mix {
            id: m.id,
            name: m.name,
            description: m.description,
            image: m.image,
            track_count: m.tracks.len(),
        }
    }
}

/// Homepage store - manages entries per user
pub struct HomepageStore {
    /// Entries per user ID
    entries: RwLock<HashMap<i64, Vec<HomepageEntry>>>,
    /// Default entries (for guests or new users)
    default_entries: RwLock<Vec<HomepageEntry>>,
}

impl HomepageStore {
    pub fn get() -> Arc<HomepageStore> {
        HOMEPAGE_STORE
            .get_or_init(|| {
                Arc::new(HomepageStore {
                    entries: RwLock::new(HashMap::new()),
                    default_entries: RwLock::new(Vec::new()),
                })
            })
            .clone()
    }

    /// Initialize default entries
    pub async fn init(&self) {
        let mut entries = Vec::new();

        // Recently added albums
        let albums = AlbumStore::get().get_all();
        let mut sorted_albums = albums;
        sorted_albums.sort_by(|a, b| b.created_date.cmp(&a.created_date));

        if !sorted_albums.is_empty() {
            entries.push(HomepageEntry {
                id: "recently-added-albums".to_string(),
                entry_type: EntryType::RecentlyAdded,
                title: "Recently Added".to_string(),
                description: "New albums in your library".to_string(),
                items: sorted_albums
                    .into_iter()
                    .take(10)
                    .map(HomepageItem::from)
                    .collect(),
                visible: true,
                order: 0,
            });
        }

        // Popular artists
        let artists = ArtistStore::get().get_all();
        let mut sorted_artists = artists;
        sorted_artists.sort_by(|a, b| b.trackcount.cmp(&a.trackcount));

        if !sorted_artists.is_empty() {
            entries.push(HomepageEntry {
                id: "popular-artists".to_string(),
                entry_type: EntryType::Artists,
                title: "Popular Artists".to_string(),
                description: "Artists with the most tracks".to_string(),
                items: sorted_artists
                    .into_iter()
                    .take(10)
                    .map(HomepageItem::from)
                    .collect(),
                visible: true,
                order: 1,
            });
        }

        // Get mixes
        let mixes = Recipes::get_homepage_mixes().await;
        for (i, mix) in mixes.into_iter().enumerate() {
            entries.push(HomepageEntry {
                id: mix.id.clone(),
                entry_type: EntryType::Mix,
                title: mix.name.clone(),
                description: mix.description.clone(),
                items: vec![HomepageItem::from(mix)],
                visible: true,
                order: (i + 2) as i32,
            });
        }

        *self.default_entries.write() = entries;
    }

    /// Get entries for a user
    pub fn get_entries(&self, user_id: i64) -> Vec<HomepageEntry> {
        let entries = self.entries.read();
        entries
            .get(&user_id)
            .cloned()
            .unwrap_or_else(|| self.default_entries.read().clone())
    }

    /// Set entries for a user
    pub fn set_entries(&self, user_id: i64, entries: Vec<HomepageEntry>) {
        self.entries.write().insert(user_id, entries);
    }

    /// Add entry for user
    pub fn add_entry(&self, user_id: i64, entry: HomepageEntry) {
        let mut entries = self.entries.write();
        let user_entries = entries.entry(user_id).or_insert_with(Vec::new);
        user_entries.push(entry);
    }

    /// Remove entry for user
    pub fn remove_entry(&self, user_id: i64, entry_id: &str) {
        let mut entries = self.entries.write();
        if let Some(user_entries) = entries.get_mut(&user_id) {
            user_entries.retain(|e| e.id != entry_id);
        }
    }

    /// Reorder entries for user
    pub fn reorder_entries(&self, user_id: i64, entry_ids: &[String]) {
        let mut entries = self.entries.write();
        if let Some(user_entries) = entries.get_mut(&user_id) {
            for (i, id) in entry_ids.iter().enumerate() {
                if let Some(entry) = user_entries.iter_mut().find(|e| &e.id == id) {
                    entry.order = i as i32;
                }
            }
            user_entries.sort_by_key(|e| e.order);
        }
    }

    /// Update recently played for user
    pub async fn update_recently_played(&self, user_id: i64) {
        let tracks = Recipes::recently_played(20).await;

        if tracks.is_empty() {
            return;
        }

        let entry = HomepageEntry {
            id: "recently-played".to_string(),
            entry_type: EntryType::RecentlyPlayed,
            title: "Recently Played".to_string(),
            description: "Continue listening".to_string(),
            items: tracks.into_iter().map(HomepageItem::from).collect(),
            visible: true,
            order: -1, // Always first
        };

        let mut entries = self.entries.write();
        let user_entries = entries
            .entry(user_id)
            .or_insert_with(|| self.default_entries.read().clone());

        // Replace or insert recently played
        if let Some(existing) = user_entries.iter_mut().find(|e| e.id == "recently-played") {
            *existing = entry;
        } else {
            user_entries.insert(0, entry);
        }
    }

    /// Clear user entries
    pub fn clear_user(&self, user_id: i64) {
        self.entries.write().remove(&user_id);
    }
}
