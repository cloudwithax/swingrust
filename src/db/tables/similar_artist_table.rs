//! Similar artist table operations
//!
//! reads from userdata.db notlastfm_similar_artists table to match upstream

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::db::UserdataEngine;

/// similar artist entry matching upstream format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarArtistData {
    pub artisthash: String,
    pub name: String,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub scrobbles: i64,
    #[serde(default)]
    pub listeners: i64,
}

/// similar artist table operations
pub struct SimilarArtistTable;

impl SimilarArtistTable {
    /// get similar artists for an artist from the userdata.db
    /// returns a set of artist hashes that are similar to the given artist
    pub async fn get_similar(artisthash: &str) -> Result<Vec<String>> {
        let engine = UserdataEngine::get()?;
        let pool = engine.pool();

        // query the notlastfm_similar_artists table which stores similar_artists as json
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT similar_artists FROM notlastfm_similar_artists WHERE artisthash = ?",
        )
        .bind(artisthash)
        .fetch_optional(pool)
        .await?;

        match row {
            Some((json_str,)) => {
                // parse the json array of similar artists
                let similar: Vec<SimilarArtistData> =
                    serde_json::from_str(&json_str).unwrap_or_default();
                Ok(similar.into_iter().map(|s| s.artisthash).collect())
            }
            None => Ok(Vec::new()),
        }
    }

    /// get the full similar artist data including weights
    pub async fn get_similar_full(artisthash: &str) -> Result<Vec<SimilarArtistData>> {
        let engine = UserdataEngine::get()?;
        let pool = engine.pool();

        let row: Option<(String,)> = sqlx::query_as(
            "SELECT similar_artists FROM notlastfm_similar_artists WHERE artisthash = ?",
        )
        .bind(artisthash)
        .fetch_optional(pool)
        .await?;

        match row {
            Some((json_str,)) => {
                let similar: Vec<SimilarArtistData> =
                    serde_json::from_str(&json_str).unwrap_or_default();
                Ok(similar)
            }
            None => Ok(Vec::new()),
        }
    }

    /// get a set of similar artist hashes
    pub async fn get_similar_hashes(artisthash: &str) -> Result<HashSet<String>> {
        let hashes = Self::get_similar(artisthash).await?;
        Ok(hashes.into_iter().collect())
    }

    /// check if similar exists for an artist
    pub async fn exists(artisthash: &str) -> Result<bool> {
        let engine = UserdataEngine::get()?;
        let pool = engine.pool();

        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM notlastfm_similar_artists WHERE artisthash = ? LIMIT 1",
        )
        .bind(artisthash)
        .fetch_optional(pool)
        .await?;

        Ok(row.is_some())
    }

    /// insert similar artists for an artist
    pub async fn insert(artisthash: &str, similar: &[SimilarArtistData]) -> Result<i64> {
        let engine = UserdataEngine::get()?;
        let pool = engine.pool();

        let json_str = serde_json::to_string(similar)?;

        let result = sqlx::query(
            "INSERT OR REPLACE INTO notlastfm_similar_artists (artisthash, similar_artists) VALUES (?, ?)",
        )
        .bind(artisthash)
        .bind(&json_str)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// delete similar artists for an artist
    pub async fn delete(artisthash: &str) -> Result<()> {
        let engine = UserdataEngine::get()?;
        let pool = engine.pool();

        sqlx::query("DELETE FROM notlastfm_similar_artists WHERE artisthash = ?")
            .bind(artisthash)
            .execute(pool)
            .await?;

        Ok(())
    }
}
