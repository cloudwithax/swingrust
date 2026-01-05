//! Homepage store - in-memory storage for homepage sections

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};

/// Global homepage store instance
static HOMEPAGE_STORE: OnceLock<Arc<HomepageStore>> = OnceLock::new();

/// Homepage section data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomepageSection {
    pub id: String,
    pub title: String,
    pub section_type: String,
    pub items: Vec<String>, // Hashes of items (tracks, albums, artists)
    pub order_index: i32,
    pub active: bool,
}

/// Homepage item type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HomepageItemType {
    Track(String),
    Album(String),
    Artist(String),
    Mix(i64),
}

/// In-memory store for homepage data
pub struct HomepageStore {
    /// All sections by id
    sections: RwLock<HashMap<String, HomepageSection>>,
    /// Section order
    section_order: RwLock<Vec<String>>,
    /// Recently played tracks
    recently_played: RwLock<Vec<String>>,
    /// Recently added albums
    recently_added: RwLock<Vec<String>>,
}

impl HomepageStore {
    /// Get or initialize the global homepage store
    pub fn get() -> Arc<HomepageStore> {
        HOMEPAGE_STORE
            .get_or_init(|| {
                Arc::new(HomepageStore {
                    sections: RwLock::new(HashMap::new()),
                    section_order: RwLock::new(Vec::new()),
                    recently_played: RwLock::new(Vec::new()),
                    recently_added: RwLock::new(Vec::new()),
                })
            })
            .clone()
    }

    /// Load sections from database
    pub fn load(&self, sections: Vec<HomepageSection>) {
        let mut section_map = self.sections.write().unwrap();
        let mut order = self.section_order.write().unwrap();

        section_map.clear();
        order.clear();

        // Sort by order_index
        let mut sorted_sections = sections;
        sorted_sections.sort_by_key(|s| s.order_index);

        for section in sorted_sections {
            order.push(section.id.clone());
            section_map.insert(section.id.clone(), section);
        }
    }

    /// Get all sections in order
    pub fn get_all_sections(&self) -> Vec<HomepageSection> {
        let section_map = self.sections.read().unwrap();
        let order = self.section_order.read().unwrap();

        order
            .iter()
            .filter_map(|id| section_map.get(id).cloned())
            .filter(|s| s.active)
            .collect()
    }

    /// Get section by id
    pub fn get_section(&self, id: &str) -> Option<HomepageSection> {
        self.sections.read().unwrap().get(id).cloned()
    }

    /// Add or update section
    pub fn upsert_section(&self, section: HomepageSection) {
        let id = section.id.clone();
        let mut section_map = self.sections.write().unwrap();
        let mut order = self.section_order.write().unwrap();

        if !section_map.contains_key(&id) {
            order.push(id.clone());
        }
        section_map.insert(id, section);
    }

    /// Remove section
    pub fn remove_section(&self, id: &str) {
        self.sections.write().unwrap().remove(id);
        self.section_order.write().unwrap().retain(|s| s != id);
    }

    /// Set recently played tracks
    pub fn set_recently_played(&self, track_hashes: Vec<String>) {
        *self.recently_played.write().unwrap() = track_hashes;
    }

    /// Get recently played tracks
    pub fn get_recently_played(&self) -> Vec<String> {
        self.recently_played.read().unwrap().clone()
    }

    /// Add to recently played
    pub fn add_recently_played(&self, track_hash: String, max_items: usize) {
        let mut recently = self.recently_played.write().unwrap();
        recently.retain(|h| h != &track_hash);
        recently.insert(0, track_hash);
        if recently.len() > max_items {
            recently.truncate(max_items);
        }
    }

    /// Set recently added albums
    pub fn set_recently_added(&self, album_hashes: Vec<String>) {
        *self.recently_added.write().unwrap() = album_hashes;
    }

    /// Get recently added albums
    pub fn get_recently_added(&self) -> Vec<String> {
        self.recently_added.read().unwrap().clone()
    }

    /// Add to recently added
    pub fn add_recently_added(&self, album_hash: String, max_items: usize) {
        let mut recently = self.recently_added.write().unwrap();
        recently.retain(|h| h != &album_hash);
        recently.insert(0, album_hash);
        if recently.len() > max_items {
            recently.truncate(max_items);
        }
    }

    /// Reorder sections
    pub fn reorder_sections(&self, section_ids: Vec<String>) {
        *self.section_order.write().unwrap() = section_ids;
    }

    /// Clear the store
    pub fn clear(&self) {
        self.sections.write().unwrap().clear();
        self.section_order.write().unwrap().clear();
        self.recently_played.write().unwrap().clear();
        self.recently_added.write().unwrap().clear();
    }
}
