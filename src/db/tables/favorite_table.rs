//! Favorite table operations

use anyhow::Result;
use serde_json::Value;
use sqlx::FromRow;

use crate::db::DbEngine;
use crate::models::{Favorite, FavoriteType};

/// Database row for favorite table
#[derive(Debug, FromRow)]
struct FavoriteRow {
    id: i64,
    hash: String,
    #[sqlx(rename = "type")]
    fav_type: String,
    timestamp: i64,
    userid: i64,
    extra: String,
}

impl FavoriteRow {
    fn into_favorite(self) -> Option<Favorite> {
        // Parse prefixed hash
        let (fav_type, hash) = if let Some((t, h)) = Favorite::parse_prefixed_hash(&self.hash) {
            (t, h)
        } else {
            (FavoriteType::from_str(&self.fav_type)?, self.hash.clone())
        };

        let extra: serde_json::Value =
            serde_json::from_str(&self.extra).unwrap_or(serde_json::Value::Null);

        Some(Favorite {
            id: self.id,
            hash,
            favorite_type: fav_type,
            timestamp: self.timestamp,
            userid: self.userid,
            extra,
        })
    }
}

/// Favorite table operations
pub struct FavoriteTable;

impl FavoriteTable {
    /// Get all favorites
    pub async fn all(userid: Option<i64>) -> Result<Vec<Favorite>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<FavoriteRow> = if let Some(uid) = userid {
            sqlx::query_as("SELECT * FROM favorite WHERE userid = ? ORDER BY timestamp DESC")
                .bind(uid)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as("SELECT * FROM favorite ORDER BY timestamp DESC")
                .fetch_all(pool)
                .await?
        };

        Ok(rows.into_iter().filter_map(|r| r.into_favorite()).collect())
    }

    /// Get favorites by type
    pub async fn get_by_type(
        fav_type: FavoriteType,
        userid: i64,
        start: i64,
        limit: i64,
    ) -> Result<Vec<Favorite>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let type_str = fav_type.as_str();
        let rows: Vec<FavoriteRow> = sqlx::query_as(
            "SELECT * FROM favorite WHERE type = ? AND userid = ? ORDER BY timestamp DESC LIMIT ? OFFSET ?"
        )
        .bind(type_str)
        .bind(userid)
        .bind(limit)
        .bind(start)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().filter_map(|r| r.into_favorite()).collect())
    }

    /// Add favorite
    pub async fn add(hash: &str, fav_type: FavoriteType, userid: i64) -> Result<i64> {
        Self::add_with_extra(hash, fav_type, userid, &serde_json::json!({})).await
    }

    /// Add favorite with custom extra payload
    pub async fn add_with_extra(
        hash: &str,
        fav_type: FavoriteType,
        userid: i64,
        extra: &Value,
    ) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let prefixed_hash = format!("{}_{}", fav_type.as_str(), hash);
        let type_str = fav_type.as_str();
        let timestamp = chrono::Utc::now().timestamp();
        let extra_json = serde_json::to_string(extra).unwrap_or_else(|_| "{}".to_string());

        let result = sqlx::query(
            "INSERT OR REPLACE INTO favorite (hash, type, timestamp, userid, extra) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&prefixed_hash)
        .bind(type_str)
        .bind(timestamp)
        .bind(userid)
        .bind(extra_json)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Remove favorite
    pub async fn remove(hash: &str, fav_type: FavoriteType, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let prefixed_hash = format!("{}_{}", fav_type.as_str(), hash);

        let result =
            sqlx::query("DELETE FROM favorite WHERE (hash = ? OR hash = ?) AND userid = ?")
                .bind(&prefixed_hash)
                .bind(hash)
                .bind(userid)
                .execute(pool)
                .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if favorite exists
    pub async fn exists(hash: &str, fav_type: FavoriteType, userid: i64) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let prefixed_hash = format!("{}_{}", fav_type.as_str(), hash);

        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM favorite WHERE (hash = ? OR hash = ?) AND userid = ?",
        )
        .bind(&prefixed_hash)
        .bind(hash)
        .bind(userid)
        .fetch_one(pool)
        .await?;

        Ok(row.0 > 0)
    }

    /// Get favorite by hash and type
    pub async fn get_by_hash(
        hash: &str,
        fav_type: FavoriteType,
        userid: i64,
    ) -> Result<Option<Favorite>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let prefixed_hash = format!("{}_{}", fav_type.as_str(), hash);

        let row: Option<FavoriteRow> =
            sqlx::query_as("SELECT * FROM favorite WHERE (hash = ? OR hash = ?) AND userid = ?")
                .bind(&prefixed_hash)
                .bind(hash)
                .bind(userid)
                .fetch_optional(pool)
                .await?;

        Ok(row.and_then(|r| r.into_favorite()))
    }

    /// Count favorites in time range
    pub async fn count_in_range(userid: i64, start: i64, end: i64) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM favorite WHERE userid = ? AND timestamp >= ? AND timestamp <= ?",
        )
        .bind(userid)
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await?;

        Ok(row.0)
    }

    /// Count favorite tracks
    pub async fn count_tracks(userid: i64) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM favorite WHERE userid = ? AND type = 'track'")
                .bind(userid)
                .fetch_one(pool)
                .await?;

        Ok(row.0)
    }

    /// Get most recent favorite track hash
    pub async fn get_recent_track_hash(userid: i64) -> Result<Option<String>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<(String,)> = sqlx::query_as(
            "SELECT hash FROM favorite WHERE userid = ? AND type = 'track' ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(userid)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|(h,)| {
            // Remove prefix if present
            if let Some((_, hash)) = Favorite::parse_prefixed_hash(&h) {
                hash
            } else {
                h
            }
        }))
    }

    /// Insert a favorite (alias for add)
    pub async fn insert(hash: &str, fav_type: FavoriteType, userid: i64) -> Result<i64> {
        Self::add(hash, fav_type, userid).await
    }

    /// Delete a favorite (alias for remove)
    pub async fn delete(hash: &str, fav_type: FavoriteType, userid: i64) -> Result<bool> {
        Self::remove(hash, fav_type, userid).await
    }

    /// Check if favorite exists (alias for exists)
    pub async fn is_favorite(hash: &str, fav_type: FavoriteType, userid: i64) -> Result<bool> {
        Self::exists(hash, fav_type, userid).await
    }
}
