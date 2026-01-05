//! LibData table operations (for colors)

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;

/// Database row for libdata table
#[derive(Debug, FromRow)]
pub struct LibDataRow {
    pub id: i64,
    pub hash: String,
    #[sqlx(rename = "type")]
    pub data_type: String,
    pub color: String,
}

/// LibData table operations
pub struct LibDataTable;

impl LibDataTable {
    /// Update or insert lib data entry
    pub async fn upsert(hash: &str, data_type: &str, color: &str) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query(
            r#"
            INSERT INTO libdata (hash, type, color)
            VALUES (?, ?, ?)
            ON CONFLICT(hash) DO UPDATE SET color = excluded.color
            "#,
        )
        .bind(hash)
        .bind(data_type)
        .bind(color)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Find by hash and type
    pub async fn find_by_hash(hash: &str, data_type: &str) -> Result<Option<String>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<(String,)> =
            sqlx::query_as("SELECT color FROM libdata WHERE hash = ? AND type = ?")
                .bind(hash)
                .bind(data_type)
                .fetch_optional(pool)
                .await?;

        Ok(row.map(|(c,)| c))
    }

    /// Get all colors for type
    pub async fn get_all_by_type(data_type: &str) -> Result<Vec<(String, String)>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT hash, color FROM libdata WHERE type = ?")
                .bind(data_type)
                .fetch_all(pool)
                .await?;

        Ok(rows)
    }
}
