//! Plugin system for SwingMusic
//!
//! This module handles loading and managing plugins that extend SwingMusic functionality.

pub mod lastfm;
pub mod lyrics;

pub use lastfm::LastFmPlugin;

#[allow(unused_imports)]
pub use lyrics::{LyricsPlugin, LyricsSearchResult, MusixmatchProvider};

use anyhow::Result;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Global plugin registry
static PLUGIN_REGISTRY: Lazy<Arc<RwLock<PluginRegistry>>> =
    Lazy::new(|| Arc::new(RwLock::new(PluginRegistry::new())));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, Plugin>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Plugin) {
        self.plugins.insert(plugin.id.clone(), plugin);
    }

    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.plugins.get(id)
    }

    pub fn get_all(&self) -> Vec<&Plugin> {
        self.plugins.values().collect()
    }

    pub fn remove(&mut self, id: &str) -> Option<Plugin> {
        self.plugins.remove(id)
    }

    pub fn enable(&mut self, id: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.enabled = true;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Plugin not found: {}", id))
        }
    }

    pub fn disable(&mut self, id: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.get_mut(id) {
            plugin.enabled = false;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Plugin not found: {}", id))
        }
    }
}

/// Register all plugins from the plugins directory
pub async fn register_plugins() -> Result<()> {
    use crate::config::Paths;
    use crate::db::DbEngine;

    let paths = Paths::get()?;
    let plugins_dir = paths.config_dir().join("plugins");

    if !plugins_dir.exists() {
        tokio::fs::create_dir_all(&plugins_dir).await?;
    }

    // Load plugins from database
    let db = DbEngine::get()?;
    let plugins = sqlx::query_as::<_, (i64, String, bool, String)>(
        "SELECT id, name, active, settings FROM plugin",
    )
    .fetch_all(db.pool())
    .await?;

    let mut registry = PLUGIN_REGISTRY.write();
    for (id, name, active, settings_json) in plugins {
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_str(&settings_json).unwrap_or_default();

        let plugin = Plugin {
            id: id.to_string(),
            name: name.clone(),
            version: String::from("1.0.0"), // Default version
            description: String::new(),
            author: String::new(),
            enabled: active,
            config,
        };

        registry.register(plugin);
    }

    Ok(())
}

/// Get the global plugin registry
pub fn get_registry() -> Arc<RwLock<PluginRegistry>> {
    PLUGIN_REGISTRY.clone()
}
