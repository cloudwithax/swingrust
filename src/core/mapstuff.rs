//! Map additional data into stores (favorites, colors, scrobbles)

use crate::db::DbEngine;
use crate::stores::{AlbumStore, ArtistStore, TrackStore};
use anyhow::Result;

/// Map favorites from database to stores
pub async fn map_favorites() -> Result<()> {
    let db = DbEngine::get()?;

    // Map track favorites
    let track_favorites =
        sqlx::query_as::<_, (String,)>("SELECT hash FROM favorite WHERE type = 'track'")
            .fetch_all(db.pool())
            .await?;

    for (trackhash,) in track_favorites {
        TrackStore::get().mark_favorite(&trackhash, true);
    }

    // Map album favorites
    let album_favorites =
        sqlx::query_as::<_, (String,)>("SELECT hash FROM favorite WHERE type = 'album'")
            .fetch_all(db.pool())
            .await?;

    for (albumhash,) in album_favorites {
        AlbumStore::get().mark_favorite(&albumhash, true);
    }

    // Map artist favorites
    let artist_favorites =
        sqlx::query_as::<_, (String,)>("SELECT hash FROM favorite WHERE type = 'artist'")
            .fetch_all(db.pool())
            .await?;

    for (artisthash,) in artist_favorites {
        ArtistStore::get().mark_favorite(&artisthash, true);
    }

    Ok(())
}

/// Map colors from database to album store
pub async fn map_colors() -> Result<()> {
    let db = DbEngine::get()?;

    let colors = sqlx::query_as::<_, (String, String)>(
        "SELECT hash, color FROM libdata WHERE type = 'album'",
    )
    .fetch_all(db.pool())
    .await?;

    for (albumhash, color) in colors {
        AlbumStore::get().set_color(&albumhash, &color);
    }

    Ok(())
}

/// Map scrobble data (play counts) to stores
pub async fn map_scrobble_data() -> Result<()> {
    let db = DbEngine::get()?;

    // Map track play counts
    let track_scrobbles = sqlx::query_as::<_, (String, i32)>(
        "SELECT trackhash, COUNT(*) as count FROM scrobble GROUP BY trackhash",
    )
    .fetch_all(db.pool())
    .await?;

    for (trackhash, count) in track_scrobbles {
        TrackStore::get().set_play_count(&trackhash, count);
    }

    Ok(())
}
