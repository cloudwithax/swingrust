//! Scrobble table operations

use anyhow::Result;
use serde_json::Value;
use sqlx::FromRow;

use crate::db::DbEngine;
use crate::models::TrackLog;

/// Database row for scrobble table
#[derive(Debug, FromRow)]
struct ScrobbleRow {
    id: i64,
    trackhash: String,
    timestamp: i64,
    duration: i32,
    source: String,
    userid: i64,
    extra: String,
}

impl ScrobbleRow {
    fn into_track_log(self) -> TrackLog {
        let mut log = TrackLog::new(
            self.trackhash,
            self.timestamp,
            self.duration,
            self.source,
            self.userid,
        );
        log.id = self.id;
        log.extra = serde_json::from_str(&self.extra).unwrap_or_default();
        log
    }
}

/// Scrobble table operations
pub struct ScrobbleTable;

impl ScrobbleTable {
    /// Insert scrobble with default source/user (compat wrapper)
    pub async fn insert(trackhash: &str, timestamp: i64, duration: i32) -> Result<i64> {
        Self::add(trackhash, timestamp, duration, "unknown", 0).await
    }

    /// Add scrobble entry
    pub async fn add(
        trackhash: &str,
        timestamp: i64,
        duration: i32,
        source: &str,
        userid: i64,
    ) -> Result<i64> {
        Self::add_with_extra(
            trackhash,
            timestamp,
            duration,
            source,
            userid,
            &serde_json::json!({}),
        )
        .await
    }

    /// Add scrobble entry with extra payload
    pub async fn add_with_extra(
        trackhash: &str,
        timestamp: i64,
        duration: i32,
        source: &str,
        userid: i64,
        extra: &Value,
    ) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let extra_json = serde_json::to_string(extra).unwrap_or_else(|_| "{}".to_string());

        let result = sqlx::query(
            "INSERT INTO scrobble (trackhash, timestamp, duration, source, userid, extra) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(trackhash)
        .bind(timestamp)
        .bind(duration)
        .bind(source)
        .bind(userid)
        .bind(extra_json)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get paginated scrobbles
    pub async fn get_paginated(userid: i64, start: i64, limit: i64) -> Result<Vec<TrackLog>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<ScrobbleRow> = sqlx::query_as(
            "SELECT * FROM scrobble WHERE userid = ? ORDER BY timestamp DESC LIMIT ? OFFSET ?",
        )
        .bind(userid)
        .bind(limit)
        .bind(start)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_track_log()).collect())
    }

    /// Get paginated scrobbles for all users (for homepage/stats)
    pub async fn get_paginated_all(start: i64, limit: i64) -> Result<Vec<TrackLog>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<ScrobbleRow> = sqlx::query_as(
            "SELECT * FROM scrobble ORDER BY timestamp DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(start)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_track_log()).collect())
    }

    /// Get paginated scrobbles for default user (compat wrapper)
    /// note: now uses get_paginated_all to support multi-user scenarios
    pub async fn get_paginated_default(start: i64, limit: i64) -> Result<Vec<TrackLog>> {
        Self::get_paginated_all(start, limit).await
    }

    /// Get scrobbles in time range
    pub async fn get_in_range(
        userid: i64,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<TrackLog>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<ScrobbleRow> = sqlx::query_as(
            "SELECT * FROM scrobble WHERE userid = ? AND timestamp >= ? AND timestamp <= ? ORDER BY timestamp DESC"
        )
        .bind(userid)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_track_log()).collect())
    }

    /// Get scrobbles in time range for default user (compat wrapper)
    pub async fn get_by_time_range(start_time: i64, end_time: i64) -> Result<Vec<TrackLog>> {
        Self::get_in_range(0, start_time, end_time).await
    }

    /// Get all scrobbles for a user
    pub async fn all(userid: i64) -> Result<Vec<TrackLog>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<ScrobbleRow> =
            sqlx::query_as("SELECT * FROM scrobble WHERE userid = ? ORDER BY timestamp DESC")
                .bind(userid)
                .fetch_all(pool)
                .await?;

        Ok(rows.into_iter().map(|r| r.into_track_log()).collect())
    }

    /// Get all scrobbles for default user (compat wrapper)
    pub async fn get_all() -> Result<Vec<TrackLog>> {
        Self::all(0).await
    }

    /// Get most recent scrobble
    pub async fn get_recent(userid: i64) -> Result<Option<TrackLog>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<ScrobbleRow> = sqlx::query_as(
            "SELECT * FROM scrobble WHERE userid = ? ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(userid)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| r.into_track_log()))
    }

    /// Get most recent scrobble for default user (compat wrapper)
    pub async fn get_most_recent() -> Result<Option<TrackLog>> {
        Self::get_recent(0).await
    }

    /// Count scrobbles in time range
    pub async fn count_in_range(userid: i64, start_time: i64, end_time: i64) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM scrobble WHERE userid = ? AND timestamp >= ? AND timestamp <= ?",
        )
        .bind(userid)
        .bind(start_time)
        .bind(end_time)
        .fetch_one(pool)
        .await?;

        Ok(row.0)
    }

    /// Get total play duration in time range
    pub async fn total_duration_in_range(
        userid: i64,
        start_time: i64,
        end_time: i64,
    ) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(duration) FROM scrobble WHERE userid = ? AND timestamp >= ? AND timestamp <= ?"
        )
        .bind(userid)
        .bind(start_time)
        .bind(end_time)
        .fetch_one(pool)
        .await?;

        Ok(row.0.unwrap_or(0))
    }
}
