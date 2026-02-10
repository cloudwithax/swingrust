//! Database engine and connection management

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

use crate::config::Paths;

static DB_ENGINE: OnceCell<Arc<DbEngine>> = OnceCell::new();

/// Database engine wrapper
pub struct DbEngine {
    pool: SqlitePool,
}

impl DbEngine {
    /// Get the global database engine instance
    pub fn get() -> Result<Arc<DbEngine>> {
        DB_ENGINE
            .get()
            .map(Arc::clone)
            .context("Database not initialized")
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Setup the SQLite database
pub async fn setup_sqlite() -> Result<()> {
    let paths = Paths::get()?;
    let db_path = paths.app_db_path();

    // Create connection options with SQLite pragmas
    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(30))
        .pragma("cache_size", "10000")
        .pragma("foreign_keys", "ON")
        .pragma("temp_store", "FILE")
        .pragma("mmap_size", "0");

    // Create connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(options)
        .await
        .context("Failed to connect to database")?;

    // Initialize the engine
    let engine = DbEngine { pool };

    DB_ENGINE
        .set(Arc::new(engine))
        .map_err(|_| anyhow::anyhow!("Database already initialized"))?;

    // Create tables
    create_tables().await?;

    Ok(())
}

/// Create all database tables
async fn create_tables() -> Result<()> {
    let engine = DbEngine::get()?;
    let pool = engine.pool();

    // Track table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS track (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            album TEXT NOT NULL,
            albumartists TEXT NOT NULL,
            albumhash TEXT NOT NULL,
            artists TEXT NOT NULL,
            bitrate INTEGER NOT NULL,
            copyright TEXT,
            date INTEGER,
            disc INTEGER NOT NULL,
            duration INTEGER NOT NULL,
            filepath TEXT NOT NULL UNIQUE,
            folder TEXT NOT NULL,
            genres TEXT,
            last_mod REAL NOT NULL,
            title TEXT NOT NULL,
            track INTEGER NOT NULL,
            trackhash TEXT NOT NULL,
            lastplayed INTEGER NOT NULL DEFAULT 0,
            playcount INTEGER NOT NULL DEFAULT 0,
            playduration INTEGER NOT NULL DEFAULT 0,
            extra TEXT DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_track_albumhash ON track(albumhash);
        CREATE INDEX IF NOT EXISTS idx_track_filepath ON track(filepath);
        CREATE INDEX IF NOT EXISTS idx_track_folder ON track(folder);
        CREATE INDEX IF NOT EXISTS idx_track_trackhash ON track(trackhash);
        "#,
    )
    .execute(pool)
    .await?;

    // User table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            image TEXT,
            password TEXT NOT NULL,
            username TEXT NOT NULL,
            roles TEXT NOT NULL DEFAULT '["user"]',
            extra TEXT DEFAULT '{}'
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_user_username ON user(username);
        "#,
    )
    .execute(pool)
    .await?;

    // Favorites table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS favorite (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            hash TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            userid INTEGER NOT NULL DEFAULT 1,
            extra TEXT DEFAULT '{}',
            FOREIGN KEY (userid) REFERENCES user(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_favorite_type ON favorite(type);
        CREATE INDEX IF NOT EXISTS idx_favorite_timestamp ON favorite(timestamp);
        CREATE INDEX IF NOT EXISTS idx_favorite_userid ON favorite(userid);
        "#,
    )
    .execute(pool)
    .await?;

    // Playlist table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS playlist (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            userid INTEGER NOT NULL,
            name TEXT NOT NULL,
            last_updated TEXT NOT NULL,
            image TEXT,
            trackhashes TEXT NOT NULL DEFAULT '[]',
            settings TEXT NOT NULL DEFAULT '{}',
            extra TEXT DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_playlist_name ON playlist(name);
        "#,
    )
    .execute(pool)
    .await?;

    // Scrobble table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS scrobble (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            trackhash TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            duration INTEGER NOT NULL,
            source TEXT NOT NULL,
            userid INTEGER NOT NULL,
            extra TEXT DEFAULT '{}',
            FOREIGN KEY (userid) REFERENCES user(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_scrobble_trackhash ON scrobble(trackhash);
        CREATE INDEX IF NOT EXISTS idx_scrobble_userid ON scrobble(userid);
        "#,
    )
    .execute(pool)
    .await?;

    // Mix table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mix (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            mixid TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            timestamp INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            trackhashes TEXT NOT NULL DEFAULT '[]',
            sourcehash TEXT NOT NULL,
            userid INTEGER NOT NULL,
            saved INTEGER NOT NULL DEFAULT 0,
            images TEXT NOT NULL DEFAULT '[]',
            extra TEXT DEFAULT '{}',
            FOREIGN KEY (userid) REFERENCES user(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_mix_sourcehash ON mix(sourcehash);
        CREATE INDEX IF NOT EXISTS idx_mix_userid ON mix(userid);
        "#,
    )
    .execute(pool)
    .await?;

    // LibData table (for colors)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS libdata (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            hash TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL,
            color TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_libdata_hash ON libdata(hash);
        CREATE INDEX IF NOT EXISTS idx_libdata_type ON libdata(type);
        "#,
    )
    .execute(pool)
    .await?;

    // Similar artists table (per-related-artist rows)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS similarartist (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            artisthash TEXT NOT NULL,
            similar_artisthash TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_similarartist_artisthash ON similarartist(artisthash);
        CREATE INDEX IF NOT EXISTS idx_similarartist_similar_hash ON similarartist(similar_artisthash);
        "#,
    )
    .execute(pool)
    .await?;

    // Plugin table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS plugin (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            active INTEGER NOT NULL DEFAULT 0,
            settings TEXT NOT NULL DEFAULT '{}',
            extra TEXT DEFAULT '{}'
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Artist data table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS artistdata (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            artisthash TEXT NOT NULL UNIQUE,
            bio TEXT NOT NULL DEFAULT '',
            image TEXT,
            color TEXT,
            similar TEXT,
            extra TEXT DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_artistdata_artisthash ON artistdata(artisthash);
        "#,
    )
    .execute(pool)
    .await?;

    // Collections table (plural) matches API expectations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS collections (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            settings TEXT NOT NULL DEFAULT '[]',
            extra_data TEXT,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
            updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );
        CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);
        "#,
    )
    .execute(pool)
    .await?;

    // Pages table (plural) matches API expectations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS pages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            page_type TEXT NOT NULL,
            page_name TEXT NOT NULL,
            page_id TEXT NOT NULL,
            order_index INTEGER NOT NULL DEFAULT 0,
            settings TEXT NOT NULL DEFAULT '{}',
            active INTEGER NOT NULL DEFAULT 1
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_pages_page_id ON pages(page_id);
        CREATE INDEX IF NOT EXISTS idx_pages_active ON pages(active);
        "#,
    )
    .execute(pool)
    .await?;

    // Migration table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS dbmigration (
            id INTEGER PRIMARY KEY,
            version INTEGER NOT NULL DEFAULT 0
        );
        INSERT OR IGNORE INTO dbmigration (id, version) VALUES (1, 0);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
