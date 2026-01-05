//! Playlist table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;
use crate::models::{Playlist, PlaylistSettings};

/// Database row for playlist table
#[derive(Debug, FromRow)]
struct PlaylistRow {
    id: i64,
    userid: i64,
    name: String,
    last_updated: String,
    image: Option<String>,
    trackhashes: String,
    settings: String,
    extra: String,
}

impl PlaylistRow {
    fn into_playlist(self) -> Playlist {
        let trackhashes: Vec<String> = serde_json::from_str(&self.trackhashes).unwrap_or_default();
        let settings: PlaylistSettings = serde_json::from_str(&self.settings).unwrap_or_default();
        let extra: serde_json::Value =
            serde_json::from_str(&self.extra).unwrap_or(serde_json::Value::Null);

        Playlist::from_db_row(
            self.id,
            self.name,
            self.image,
            self.last_updated,
            trackhashes,
            settings,
            Some(self.userid),
            extra,
        )
    }
}

/// Playlist table operations
pub struct PlaylistTable;

impl PlaylistTable {
    /// Get all playlists
    pub async fn all(userid: Option<i64>) -> Result<Vec<Playlist>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<PlaylistRow> = if let Some(uid) = userid {
            sqlx::query_as("SELECT * FROM playlist WHERE userid = ?")
                .bind(uid)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as("SELECT * FROM playlist")
                .fetch_all(pool)
                .await?
        };

        Ok(rows.into_iter().map(|r| r.into_playlist()).collect())
    }

    /// Get playlist by ID
    pub async fn get_by_id(id: i64) -> Result<Option<Playlist>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<PlaylistRow> = sqlx::query_as("SELECT * FROM playlist WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| r.into_playlist()))
    }

    /// Insert playlist
    pub async fn insert(playlist: &Playlist) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let trackhashes = serde_json::to_string(&playlist.trackhashes)?;
        let settings = serde_json::to_string(&playlist.settings)?;
        let extra = serde_json::to_string(&playlist.extra)?;

        let result = sqlx::query(
            "INSERT INTO playlist (userid, name, last_updated, image, trackhashes, settings, extra) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(playlist.userid.unwrap_or(1))
        .bind(&playlist.name)
        .bind(&playlist.last_updated)
        .bind(&playlist.image)
        .bind(&trackhashes)
        .bind(&settings)
        .bind(&extra)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Check if playlist name exists
    pub async fn name_exists(name: &str, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM playlist WHERE name = ? AND userid = ?")
                .bind(name)
                .bind(userid)
                .fetch_one(pool)
                .await?;

        Ok(row.0 > 0)
    }

    /// Add tracks to playlist
    pub async fn add_tracks(id: i64, trackhashes: &[String]) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        // Get current trackhashes
        let row: Option<(String,)> =
            sqlx::query_as("SELECT trackhashes FROM playlist WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;

        let mut current: Vec<String> = row
            .and_then(|(t,)| serde_json::from_str(&t).ok())
            .unwrap_or_default();

        for hash in trackhashes {
            if !current.contains(hash) {
                current.push(hash.clone());
            }
        }

        let new_trackhashes = serde_json::to_string(&current)?;
        let last_updated = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE playlist SET trackhashes = ?, last_updated = ? WHERE id = ?")
            .bind(&new_trackhashes)
            .bind(&last_updated)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Get playlist trackhashes
    pub async fn get_trackhashes(id: i64) -> Result<Vec<String>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<(String,)> =
            sqlx::query_as("SELECT trackhashes FROM playlist WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;

        Ok(row
            .and_then(|(t,)| serde_json::from_str(&t).ok())
            .unwrap_or_default())
    }

    /// Remove tracks by indices with hash validation
    pub async fn remove_tracks(id: i64, items: &[(usize, String)]) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let current = Self::get_trackhashes(id).await?;

        let new_trackhashes: Vec<String> = current
            .into_iter()
            .enumerate()
            .filter(|(i, hash)| !items.iter().any(|(idx, h)| idx == i && h == hash))
            .map(|(_, h)| h)
            .collect();

        let trackhashes_str = serde_json::to_string(&new_trackhashes)?;
        let last_updated = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE playlist SET trackhashes = ?, last_updated = ? WHERE id = ?")
            .bind(&trackhashes_str)
            .bind(&last_updated)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Update playlist
    pub async fn update(playlist: &Playlist) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let trackhashes = serde_json::to_string(&playlist.trackhashes)?;
        let settings = serde_json::to_string(&playlist.settings)?;
        let extra = serde_json::to_string(&playlist.extra)?;
        let last_updated = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "UPDATE playlist SET name = ?, last_updated = ?, image = ?, trackhashes = ?, settings = ?, extra = ? WHERE id = ?"
        )
        .bind(&playlist.name)
        .bind(&last_updated)
        .bind(&playlist.image)
        .bind(&trackhashes)
        .bind(&settings)
        .bind(&extra)
        .bind(playlist.id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Update playlist settings
    pub async fn update_settings(id: i64, settings: &PlaylistSettings) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let settings_str = serde_json::to_string(settings)?;

        sqlx::query("UPDATE playlist SET settings = ? WHERE id = ?")
            .bind(&settings_str)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Remove playlist image
    pub async fn remove_image(id: i64) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE playlist SET image = NULL WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Delete playlist
    pub async fn delete(id: i64, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let result = if userid > 0 {
            sqlx::query("DELETE FROM playlist WHERE id = ? AND userid = ?")
                .bind(id)
                .bind(userid)
                .execute(pool)
                .await?
        } else {
            sqlx::query("DELETE FROM playlist WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?
        };

        Ok(result.rows_affected() > 0)
    }

    /// Update playlist trackhashes
    pub async fn update_tracks(id: i64, trackhashes_json: &str) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let last_updated = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("UPDATE playlist SET trackhashes = ?, last_updated = ? WHERE id = ?")
            .bind(trackhashes_json)
            .bind(&last_updated)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }
}
