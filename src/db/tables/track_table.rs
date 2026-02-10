//! Track table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;
use crate::models::{ArtistRefItem, GenreRef, Track};

/// Database row for track table
#[derive(Debug, FromRow)]
struct TrackRow {
    id: i64,
    album: String,
    albumartists: String,
    albumhash: String,
    artists: String,
    bitrate: i32,
    copyright: Option<String>,
    date: Option<i64>,
    disc: i32,
    duration: i32,
    filepath: String,
    folder: String,
    genres: Option<String>,
    last_mod: f64,
    title: String,
    track: i32,
    trackhash: String,
    lastplayed: i64,
    playcount: i32,
    playduration: i32,
    extra: String,
}

impl TrackRow {
    fn into_track(self) -> Track {
        let albumartists: Vec<ArtistRefItem> =
            serde_json::from_str(&self.albumartists).unwrap_or_default();
        let artists: Vec<ArtistRefItem> = serde_json::from_str(&self.artists).unwrap_or_default();
        let genres: Vec<GenreRef> = self
            .genres
            .as_ref()
            .and_then(|g| serde_json::from_str(g).ok())
            .unwrap_or_default();
        let extra: serde_json::Value =
            serde_json::from_str(&self.extra).unwrap_or(serde_json::Value::Null);

        let artisthashes: Vec<String> = artists.iter().map(|a| a.artisthash.clone()).collect();
        let genrehashes: Vec<String> = genres.iter().map(|g| g.genrehash.clone()).collect();

        // Clone before moving for og_album/og_title
        let og_album = self.album.clone();
        let og_title = self.title.clone();

        Track {
            id: self.id,
            album: self.album,
            albumartists,
            albumhash: self.albumhash,
            artists,
            bitrate: self.bitrate,
            copyright: self.copyright,
            date: self.date.unwrap_or(0),
            disc: self.disc,
            duration: self.duration,
            filepath: self.filepath,
            folder: self.folder,
            genres,
            // Stored as REAL in SQLite; round/truncate to i64 seconds
            last_mod: self.last_mod as i64,
            title: self.title,
            track: self.track,
            trackhash: self.trackhash,
            extra,
            lastplayed: self.lastplayed,
            playcount: self.playcount,
            playduration: self.playduration,
            og_album,
            og_title,
            artisthashes,
            genrehashes,
            weakhash: String::new(),
            pos: None,
            image: String::new(),
            help_text: String::new(),
            score: 0.0,
            explicit: false,
            fav_userids: Default::default(),
        }
    }
}

/// Track table operations
pub struct TrackTable;

impl TrackTable {
    /// Get all tracks
    pub async fn all() -> Result<Vec<Track>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<TrackRow> = sqlx::query_as("SELECT * FROM track")
            .fetch_all(pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.into_track()).collect())
    }

    /// Insert a single track
    pub async fn insert_one(track: &Track) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let albumartists = serde_json::to_string(&track.albumartists)?;
        let artists = serde_json::to_string(&track.artists)?;
        let genres = serde_json::to_string(&track.genres)?;
        let extra = serde_json::to_string(&track.extra)?;

        let result = sqlx::query(
            r#"
            INSERT INTO track (
                album, albumartists, albumhash, artists, bitrate, copyright,
                date, disc, duration, filepath, folder, genres, last_mod,
                title, track, trackhash, lastplayed, playcount, playduration, extra
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&track.album)
        .bind(&albumartists)
        .bind(&track.albumhash)
        .bind(&artists)
        .bind(track.bitrate)
        .bind(&track.copyright)
        .bind(track.date)
        .bind(track.disc)
        .bind(track.duration)
        .bind(&track.filepath)
        .bind(&track.folder)
        .bind(&genres)
        .bind(track.last_mod)
        .bind(&track.title)
        .bind(track.track)
        .bind(&track.trackhash)
        .bind(track.lastplayed)
        .bind(track.playcount)
        .bind(track.playduration)
        .bind(&extra)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Insert multiple tracks
    pub async fn insert_many(tracks: &[Track]) -> Result<()> {
        for track in tracks {
            Self::insert_one(track).await?;
        }
        Ok(())
    }

    /// Get tracks by file paths
    pub async fn get_by_filepaths(filepaths: &[String]) -> Result<Vec<Track>> {
        if filepaths.is_empty() {
            return Ok(Vec::new());
        }

        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let placeholders: String = filepaths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("SELECT * FROM track WHERE filepath IN ({})", placeholders);

        let mut query_builder = sqlx::query_as::<_, TrackRow>(&query);
        for path in filepaths {
            query_builder = query_builder.bind(path);
        }

        let rows = query_builder.fetch_all(pool).await?;
        Ok(rows.into_iter().map(|r| r.into_track()).collect())
    }

    /// Get tracks by folder path (containing)
    pub async fn get_by_folder_containing(path: &str) -> Result<Vec<Track>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let pattern = format!("{}%", path);
        let rows: Vec<TrackRow> = sqlx::query_as("SELECT * FROM track WHERE filepath LIKE ?")
            .bind(&pattern)
            .fetch_all(pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.into_track()).collect())
    }

    /// Remove tracks by file paths
    pub async fn remove_by_filepaths(filepaths: &[String]) -> Result<u64> {
        if filepaths.is_empty() {
            return Ok(0);
        }

        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let placeholders: String = filepaths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("DELETE FROM track WHERE filepath IN ({})", placeholders);

        let mut query_builder = sqlx::query(&query);
        for path in filepaths {
            query_builder = query_builder.bind(path);
        }

        let result = query_builder.execute(pool).await?;
        Ok(result.rows_affected())
    }

    /// Update play statistics for a track
    pub async fn update_play_stats(
        trackhash: &str,
        lastplayed: i64,
        playcount: i32,
        playduration: i32,
    ) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query(
            "UPDATE track SET lastplayed = ?, playcount = ?, playduration = ? WHERE trackhash = ?",
        )
        .bind(lastplayed)
        .bind(playcount)
        .bind(playduration)
        .bind(trackhash)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get track count
    pub async fn count() -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM track")
            .fetch_one(pool)
            .await?;

        Ok(row.0)
    }

    /// Remove all tracks
    pub async fn remove_all() -> Result<u64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let result = sqlx::query("DELETE FROM track").execute(pool).await?;

        Ok(result.rows_affected())
    }
}
