//! Homepage page table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;

/// Database row for homepage pages
#[derive(Debug, FromRow)]
pub struct PageRow {
    pub id: i64,
    pub page_type: String,
    pub page_name: String,
    pub page_id: String,
    pub order_index: i32,
    pub settings: String, // JSON encoded
    pub active: bool,
}

/// Page table operations
pub struct PageTable;

impl PageTable {
    /// Insert page
    pub async fn insert(
        page_type: &str,
        page_name: &str,
        page_id: &str,
        order_index: i32,
        settings: &str,
        active: bool,
    ) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let result = sqlx::query(
            r#"
            INSERT INTO pages (page_type, page_name, page_id, order_index, settings, active)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(page_type)
        .bind(page_name)
        .bind(page_id)
        .bind(order_index)
        .bind(settings)
        .bind(active)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get page by id
    pub async fn get_by_id(id: i64) -> Result<Option<PageRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row = sqlx::query_as::<_, PageRow>(
            "SELECT id, page_type, page_name, page_id, order_index, settings, active FROM pages WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    /// Get page by page_id
    pub async fn get_by_page_id(page_id: &str) -> Result<Option<PageRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row = sqlx::query_as::<_, PageRow>(
            "SELECT id, page_type, page_name, page_id, order_index, settings, active FROM pages WHERE page_id = ?"
        )
        .bind(page_id)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    /// Get all pages
    pub async fn get_all() -> Result<Vec<PageRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows = sqlx::query_as::<_, PageRow>(
            "SELECT id, page_type, page_name, page_id, order_index, settings, active FROM pages ORDER BY order_index ASC"
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Get active pages
    pub async fn get_active() -> Result<Vec<PageRow>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows = sqlx::query_as::<_, PageRow>(
            "SELECT id, page_type, page_name, page_id, order_index, settings, active FROM pages WHERE active = 1 ORDER BY order_index ASC"
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Update page order
    pub async fn update_order(id: i64, order_index: i32) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE pages SET order_index = ? WHERE id = ?")
            .bind(order_index)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Update page settings
    pub async fn update_settings(id: i64, settings: &str) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE pages SET settings = ? WHERE id = ?")
            .bind(settings)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Toggle page active status
    pub async fn toggle_active(id: i64) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE pages SET active = NOT active WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Delete page
    pub async fn delete(id: i64) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("DELETE FROM pages WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }
}
