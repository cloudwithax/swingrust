//! userdata database engine
//!
//! this module handles connections to the userdata.db which stores user-specific
//! data like similar artists, favorites, playlists, etc.

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

use crate::config::Paths;

static USERDATA_ENGINE: OnceCell<Arc<UserdataEngine>> = OnceCell::new();

/// userdata database engine wrapper
pub struct UserdataEngine {
    pool: SqlitePool,
}

impl UserdataEngine {
    /// get the global userdata engine instance
    pub fn get() -> Result<Arc<UserdataEngine>> {
        USERDATA_ENGINE
            .get()
            .map(Arc::clone)
            .context("Userdata database not initialized")
    }

    /// get a reference to the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// setup the userdata sqlite database
pub async fn setup_userdata() -> Result<()> {
    let paths = Paths::get()?;
    let db_path = paths.userdata_db_path();

    // create connection options
    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(30))
        .pragma("cache_size", "5000")
        .pragma("foreign_keys", "ON");

    // create connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(options)
        .await
        .context("Failed to connect to userdata database")?;

    // initialize the engine
    let engine = UserdataEngine { pool };

    USERDATA_ENGINE
        .set(Arc::new(engine))
        .map_err(|_| anyhow::anyhow!("Userdata database already initialized"))?;

    // create tables
    create_userdata_tables().await?;

    Ok(())
}

/// create userdata tables that match the upstream python implementation
async fn create_userdata_tables() -> Result<()> {
    let engine = UserdataEngine::get()?;
    let pool = engine.pool();

    // similar artists table matching upstream notlastfm_similar_artists
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS notlastfm_similar_artists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            artisthash TEXT NOT NULL UNIQUE,
            similar_artists TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_similar_artisthash ON notlastfm_similar_artists(artisthash);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
