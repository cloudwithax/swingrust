//! Plugin table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;

/// Database row for plugins
#[derive(Debug, FromRow)]
pub struct PluginRow {
    pub id: i64,
    pub name: String,
    pub settings: String, // JSON encoded
    pub active: bool,
}

/// Plugin table operations
pub struct PluginTable;

impl PluginTable {
    /// Insert or update plugin
    pub async fn upsert(name: &str, settings: &str, active: bool) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query(
            r#"
            INSERT INTO plugin (name, settings, active)
            VALUES (?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET settings = excluded.settings, active = excluded.active
            "#,
        )
        .bind(name)
        .bind(settings)
        .bind(active)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get plugin by name
    pub async fn get_by_name(name: &str) -> Result<Option<PluginRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row = sqlx::query_as::<_, PluginRow>(
            "SELECT id, name, settings, active FROM plugin WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    /// Get all plugins
    pub async fn get_all() -> Result<Vec<PluginRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows = sqlx::query_as::<_, PluginRow>("SELECT id, name, settings, active FROM plugin")
            .fetch_all(pool)
            .await?;

        Ok(rows)
    }

    /// Update plugin settings
    pub async fn update_settings(name: &str, settings: &str) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE plugin SET settings = ? WHERE name = ?")
            .bind(settings)
            .bind(name)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Set plugin active status
    pub async fn set_active(name: &str, active: bool) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE plugin SET active = ? WHERE name = ?")
            .bind(active)
            .bind(name)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Delete plugin
    pub async fn delete(name: &str) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("DELETE FROM plugin WHERE name = ?")
            .bind(name)
            .execute(pool)
            .await?;

        Ok(())
    }
}
