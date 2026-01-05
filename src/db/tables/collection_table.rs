//! Collection table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;

/// Database row for collections
#[derive(Debug, FromRow)]
pub struct CollectionRow {
    pub id: i64,
    pub name: String,
    pub settings: String,           // JSON encoded settings
    pub extra_data: Option<String>, // JSON encoded extra data
    pub created_at: i64,
    pub updated_at: i64,
}

/// Collection table operations
pub struct CollectionTable;

impl CollectionTable {
    /// Insert collection
    pub async fn insert(name: &str, settings: &str, extra_data: Option<&str>) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let now = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            r#"
            INSERT INTO collections (name, settings, extra_data, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(name)
        .bind(settings)
        .bind(extra_data)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get collection by id
    pub async fn get_by_id(id: i64) -> Result<Option<CollectionRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row = sqlx::query_as::<_, CollectionRow>(
            "SELECT id, name, settings, extra_data, created_at, updated_at FROM collections WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    /// Get all collections
    pub async fn get_all() -> Result<Vec<CollectionRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows = sqlx::query_as::<_, CollectionRow>(
            "SELECT id, name, settings, extra_data, created_at, updated_at FROM collections ORDER BY created_at DESC"
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Update collection
    pub async fn update(
        id: i64,
        name: Option<&str>,
        settings: Option<&str>,
        extra_data: Option<&str>,
    ) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let now = chrono::Utc::now().timestamp();

        if let Some(n) = name {
            sqlx::query("UPDATE collections SET name = ?, updated_at = ? WHERE id = ?")
                .bind(n)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
        }

        if let Some(s) = settings {
            sqlx::query("UPDATE collections SET settings = ?, updated_at = ? WHERE id = ?")
                .bind(s)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
        }

        if let Some(e) = extra_data {
            sqlx::query("UPDATE collections SET extra_data = ?, updated_at = ? WHERE id = ?")
                .bind(e)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
        }

        Ok(())
    }

    /// Delete collection
    pub async fn delete(id: i64) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }
}
