//! Mix table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;
use crate::models::Mix;

/// Database row for mix table
#[derive(Debug, FromRow)]
struct MixRow {
    id: i64,
    mixid: String,
    title: String,
    description: String,
    timestamp: i64,
    trackhashes: String,
    sourcehash: String,
    userid: i64,
    saved: i32,
    images: String,
    extra: String,
}

impl MixRow {
    fn into_mix(self) -> Mix {
        let trackhashes: Vec<String> = serde_json::from_str(&self.trackhashes).unwrap_or_default();
        let images: Vec<String> = serde_json::from_str(&self.images).unwrap_or_default();
        let extra: serde_json::Value =
            serde_json::from_str(&self.extra).unwrap_or(serde_json::Value::Null);

        Mix::from_db_row(
            self.id,
            self.timestamp,
            self.mixid,
            self.title,
            self.description,
            trackhashes,
            self.sourcehash,
            self.userid,
            self.saved != 0,
            images,
            extra,
        )
    }
}

/// Mix table operations
pub struct MixTable;

impl MixTable {
    /// Get all mixes
    pub async fn all(userid: i64) -> Result<Vec<Mix>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<MixRow> =
            sqlx::query_as("SELECT * FROM mix WHERE userid = ? ORDER BY timestamp DESC")
                .bind(userid)
                .fetch_all(pool)
                .await?;

        Ok(rows.into_iter().map(|r| r.into_mix()).collect())
    }

    /// Get mix by source hash
    pub async fn get_by_sourcehash(sourcehash: &str, userid: i64) -> Result<Option<Mix>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<MixRow> =
            sqlx::query_as("SELECT * FROM mix WHERE sourcehash = ? AND userid = ?")
                .bind(sourcehash)
                .bind(userid)
                .fetch_optional(pool)
                .await?;

        Ok(row.map(|r| r.into_mix()))
    }

    /// Get mix by mix ID
    pub async fn get_by_mixid(mixid: &str, userid: i64) -> Result<Option<Mix>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<MixRow> =
            sqlx::query_as("SELECT * FROM mix WHERE mixid = ? AND userid = ?")
                .bind(mixid)
                .bind(userid)
                .fetch_optional(pool)
                .await?;

        Ok(row.map(|r| r.into_mix()))
    }

    /// Insert mix (upsert)
    pub async fn insert(mix: &Mix) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let trackhashes = serde_json::to_string(&mix.trackhashes)?;
        let images = serde_json::to_string(&mix.images)?;
        let extra = serde_json::to_string(&mix.extra)?;

        let result = sqlx::query(
            r#"
            INSERT INTO mix (mixid, title, description, trackhashes, sourcehash, userid, saved, images, extra, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(mixid) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                trackhashes = excluded.trackhashes,
                images = excluded.images,
                timestamp = excluded.timestamp,
                extra = excluded.extra
            "#
        )
        .bind(&mix.mixid)
        .bind(&mix.title)
        .bind(&mix.description)
        .bind(&trackhashes)
        .bind(&mix.sourcehash)
        .bind(mix.userid)
        .bind(mix.saved as i32)
        .bind(&images)
        .bind(&extra)
        .bind(mix.timestamp)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Update mix
    pub async fn update(mix: &Mix) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let trackhashes = serde_json::to_string(&mix.trackhashes)?;
        let images = serde_json::to_string(&mix.images)?;
        let extra = serde_json::to_string(&mix.extra)?;

        sqlx::query(
            "UPDATE mix SET title = ?, description = ?, trackhashes = ?, images = ?, extra = ?, timestamp = ? WHERE id = ?"
        )
        .bind(&mix.title)
        .bind(&mix.description)
        .bind(&trackhashes)
        .bind(&images)
        .bind(&extra)
        .bind(mix.timestamp)
        .bind(mix.id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Toggle saved state
    pub async fn toggle_saved(mixid: &str, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        // Get current state
        let row: Option<(i32,)> =
            sqlx::query_as("SELECT saved FROM mix WHERE mixid = ? AND userid = ?")
                .bind(mixid)
                .bind(userid)
                .fetch_optional(pool)
                .await?;

        let new_state = row.map(|(s,)| s == 0).unwrap_or(true);

        sqlx::query("UPDATE mix SET saved = ? WHERE mixid = ? AND userid = ?")
            .bind(new_state as i32)
            .bind(mixid)
            .bind(userid)
            .execute(pool)
            .await?;

        Ok(new_state)
    }

    /// Get saved mixes
    pub async fn get_saved(userid: i64) -> Result<Vec<Mix>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<MixRow> = sqlx::query_as("SELECT * FROM mix WHERE userid = ? AND saved = 1")
            .bind(userid)
            .fetch_all(pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.into_mix()).collect())
    }

    /// Toggle saved flag for an artist mix by sourcehash
    pub async fn save_artist_mix(sourcehash: &str, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<(i32,)> =
            sqlx::query_as("SELECT saved FROM mix WHERE sourcehash = ? AND userid = ?")
                .bind(sourcehash)
                .bind(userid)
                .fetch_optional(pool)
                .await?;

        let new_state = row.map(|(s,)| s == 0).unwrap_or(true);

        sqlx::query("UPDATE mix SET saved = ? WHERE sourcehash = ? AND userid = ?")
            .bind(new_state as i32)
            .bind(sourcehash)
            .bind(userid)
            .execute(pool)
            .await?;

        Ok(new_state)
    }

    /// Toggle track mix saved flag stored in extra.trackmix_saved
    pub async fn save_track_mix(sourcehash: &str, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<(String,)> =
            sqlx::query_as("SELECT extra FROM mix WHERE sourcehash = ? AND userid = ?")
                .bind(sourcehash)
                .bind(userid)
                .fetch_optional(pool)
                .await?;

        if row.is_none() {
            return Ok(false);
        }

        let (extra_str,) = row.unwrap();
        let mut extra: serde_json::Value =
            serde_json::from_str(&extra_str).unwrap_or_else(|_| serde_json::json!({}));

        let mut state = extra
            .get("trackmix_saved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        state = !state;

        if let Some(obj) = extra.as_object_mut() {
            obj.insert("trackmix_saved".to_string(), serde_json::Value::Bool(state));
        } else {
            extra = serde_json::json!({ "trackmix_saved": state });
        }

        let extra_json = serde_json::to_string(&extra)?;

        sqlx::query("UPDATE mix SET extra = ? WHERE sourcehash = ? AND userid = ?")
            .bind(extra_json)
            .bind(sourcehash)
            .bind(userid)
            .execute(pool)
            .await?;

        Ok(state)
    }
}
