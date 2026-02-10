//! Plugin models

use serde::{Deserialize, Serialize};

/// Plugin settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSettings {
    #[serde(flatten)]
    pub settings: serde_json::Value,
}

/// A plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Database ID
    pub id: i64,
    /// Plugin name
    pub name: String,
    /// Is plugin active
    pub active: bool,
    /// Plugin settings
    #[serde(default)]
    pub settings: PluginSettings,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Plugin {
    pub fn new(name: String) -> Self {
        Self {
            id: 0,
            name,
            active: false,
            settings: PluginSettings::default(),
            extra: serde_json::Value::Null,
        }
    }
}

impl Default for Plugin {
    fn default() -> Self {
        Self::new(String::new())
    }
}
