//! Database migrations

use anyhow::Result;
use tracing::info;

use super::DbEngine;

/// Current migration version
const CURRENT_VERSION: i32 = 2;

/// Run database migrations
pub async fn run_migrations() -> Result<()> {
    let engine = DbEngine::get()?;
    let pool = engine.pool();

    // Get current version
    let row: (i32,) = sqlx::query_as("SELECT version FROM dbmigration WHERE id = 1")
        .fetch_one(pool)
        .await?;
    let current_version = row.0;

    if current_version >= CURRENT_VERSION {
        info!("Database is up to date (version {})", current_version);
        return Ok(());
    }

    info!(
        "Running migrations from version {} to {}",
        current_version, CURRENT_VERSION
    );

    // Run migrations in order
    for version in (current_version + 1)..=CURRENT_VERSION {
        run_migration(version).await?;

        // Update version
        sqlx::query("UPDATE dbmigration SET version = ? WHERE id = 1")
            .bind(version)
            .execute(pool)
            .await?;

        info!("Applied migration {}", version);
    }

    Ok(())
}

async fn run_migration(version: i32) -> Result<()> {
    let engine = DbEngine::get()?;
    let pool = engine.pool();

    match version {
        1 => {
            // Initial migration - tables already created in setup_sqlite
            // This is a placeholder for future migrations
        }
        2 => {
            // add timestamp column to mix table if missing
            let has_column: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info('mix') WHERE name = 'timestamp'",
            )
            .fetch_one(pool)
            .await
            .unwrap_or(1);

            if has_column == 0 {
                sqlx::query(
                    "ALTER TABLE mix ADD COLUMN timestamp INTEGER NOT NULL DEFAULT (strftime('%s','now'))",
                )
                .execute(pool)
                .await?;
            }
        }
        _ => {
            tracing::warn!("Unknown migration version: {}", version);
        }
    }

    Ok(())
}

/// Get the current migration version
pub async fn get_migration_version() -> Result<i32> {
    let engine = DbEngine::get()?;
    let pool = engine.pool();

    let row: (i32,) = sqlx::query_as("SELECT version FROM dbmigration WHERE id = 1")
        .fetch_one(pool)
        .await?;

    Ok(row.0)
}
