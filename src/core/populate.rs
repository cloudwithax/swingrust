//! Populate stores from database/index data

use anyhow::Result;

use crate::config::UserConfig;
use crate::core::{AlbumLib, ArtistLib};
use crate::db::tables::TrackTable;
use crate::models::Track;
use crate::stores::{AlbumStore, ArtistStore, FolderStore, TrackStore};

/// Populate all in-memory stores from database
pub async fn populate_stores() -> Result<()> {
    tracing::info!("Populating stores from database...");

    // Load tracks from database
    let tracks = TrackTable::all().await?;

    tracing::info!("Loaded {} tracks from database", tracks.len());

    // Populate track store
    TrackStore::get().load(tracks.clone());

    // Build and populate albums
    let albums = AlbumLib::build_albums(&tracks);
    tracing::info!("Built {} albums", albums.len());
    AlbumStore::get().load(albums);

    // Build and populate artists
    let artists = ArtistLib::build_artists(&tracks);
    tracing::info!("Built {} artists", artists.len());
    ArtistStore::get().load(artists);

    // Build folder structure
    let config = UserConfig::load()?;
    let track_folders: Vec<String> = tracks.iter().map(|t| t.folder.clone()).collect();
    FolderStore::get().load_from_paths(track_folders, &config.root_dirs);

    tracing::info!("Store population complete");

    Ok(())
}

/// Refresh stores with new tracks (incremental update)
pub fn refresh_with_tracks(new_tracks: Vec<Track>) {
    let track_store = TrackStore::get();

    for track in &new_tracks {
        track_store.add(track.clone());
    }

    // Rebuild albums with all tracks
    let all_tracks = track_store.get_all();
    let albums = AlbumLib::build_albums(&all_tracks);
    AlbumStore::get().load(albums);

    // Rebuild artists with all tracks
    let artists = ArtistLib::build_artists(&all_tracks);
    ArtistStore::get().load(artists);
}

/// Remove tracks from stores
pub fn remove_tracks(paths: &[String]) {
    TrackStore::get().remove_by_paths(paths);

    // Rebuild albums and artists
    let tracks = TrackStore::get().get_all();

    let albums = AlbumLib::build_albums(&tracks);
    AlbumStore::get().load(albums);

    let artists = ArtistLib::build_artists(&tracks);
    ArtistStore::get().load(artists);
}

/// Clear all stores
pub fn clear_stores() {
    TrackStore::get().clear();
    AlbumStore::get().clear();
    ArtistStore::get().clear();
    FolderStore::get().clear();
}

/// Get library statistics
pub fn get_stats() -> LibraryStats {
    LibraryStats {
        track_count: TrackStore::get().count(),
        album_count: AlbumStore::get().count(),
        artist_count: ArtistStore::get().count(),
        total_duration: TrackStore::get()
            .get_all()
            .iter()
            .map(|t| t.duration as i64)
            .sum(),
    }
}

/// Library statistics
pub struct LibraryStats {
    pub track_count: usize,
    pub album_count: usize,
    pub artist_count: usize,
    pub total_duration: i64,
}
