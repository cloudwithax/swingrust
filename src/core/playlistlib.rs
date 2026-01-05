//! Playlist library functions

use anyhow::Result;

use crate::db::tables::PlaylistTable;
use crate::models::{Playlist, Track};
use crate::stores::TrackStore;

/// Playlist library functions
pub struct PlaylistLib;

impl PlaylistLib {
    /// Get all playlists
    pub async fn get_all() -> Result<Vec<Playlist>> {
        PlaylistTable::all(None).await
    }

    /// Get playlist by id
    pub async fn get_by_id(id: i64) -> Result<Option<Playlist>> {
        PlaylistTable::get_by_id(id).await
    }

    /// Create new playlist
    pub async fn create(name: &str, description: Option<&str>) -> Result<i64> {
        let mut playlist = Playlist::new(name.to_string(), None);
        if let Some(desc) = description {
            playlist.extra = serde_json::json!({ "description": desc });
        }
        PlaylistTable::insert(&playlist).await
    }

    /// Update playlist metadata
    pub async fn update(id: i64, name: Option<&str>, description: Option<&str>) -> Result<()> {
        if let Some(mut playlist) = PlaylistTable::get_by_id(id).await? {
            if let Some(n) = name {
                playlist.name = n.to_string();
            }
            if let Some(d) = description {
                playlist.extra = serde_json::json!({ "description": d });
            }
            PlaylistTable::update(&playlist).await
        } else {
            Err(anyhow::anyhow!("Playlist not found"))
        }
    }

    /// Delete playlist
    pub async fn delete(id: i64) -> Result<()> {
        PlaylistTable::delete(id, 0).await.map(|_| ())
    }

    /// Get playlist tracks
    pub async fn get_tracks(playlist_id: i64) -> Result<Vec<Track>> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        match playlist {
            Some(p) => {
                let store = TrackStore::get();
                Ok(store.get_by_hashes(&p.trackhashes))
            }
            None => Ok(Vec::new()),
        }
    }

    /// Add track to playlist
    pub async fn add_track(playlist_id: i64, track_hash: &str) -> Result<()> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        if let Some(p) = playlist {
            let mut playlist = p;
            if !playlist.trackhashes.contains(&track_hash.to_string()) {
                playlist.trackhashes.push(track_hash.to_string());
                PlaylistTable::update(&playlist).await?;
            }
        }

        Ok(())
    }

    /// Add multiple tracks to playlist
    pub async fn add_tracks(playlist_id: i64, track_hashes: &[String]) -> Result<()> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        if let Some(p) = playlist {
            let mut playlist = p;
            for hash in track_hashes {
                if !playlist.trackhashes.contains(hash) {
                    playlist.trackhashes.push(hash.clone());
                }
            }
            PlaylistTable::update(&playlist).await?;
        }

        Ok(())
    }

    /// Remove track from playlist
    pub async fn remove_track(playlist_id: i64, track_hash: &str) -> Result<()> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        if let Some(p) = playlist {
            let mut playlist = p;
            playlist.trackhashes.retain(|h| h != track_hash);
            PlaylistTable::update(&playlist).await?;
        }

        Ok(())
    }

    /// Remove track at index from playlist
    pub async fn remove_track_at(playlist_id: i64, index: usize) -> Result<()> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        if let Some(p) = playlist {
            let mut playlist = p;
            if index < playlist.trackhashes.len() {
                playlist.trackhashes.remove(index);
                PlaylistTable::update(&playlist).await?;
            }
        }

        Ok(())
    }

    /// Reorder playlist tracks
    pub async fn reorder(playlist_id: i64, from_index: usize, to_index: usize) -> Result<()> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        if let Some(p) = playlist {
            let mut playlist = p;
            if from_index < playlist.trackhashes.len() && to_index < playlist.trackhashes.len() {
                let track = playlist.trackhashes.remove(from_index);
                playlist.trackhashes.insert(to_index, track);
                PlaylistTable::update(&playlist).await?;
            }
        }

        Ok(())
    }

    /// Set playlist tracks (replace all)
    pub async fn set_tracks(playlist_id: i64, track_hashes: &[String]) -> Result<()> {
        if let Some(mut playlist) = PlaylistTable::get_by_id(playlist_id).await? {
            playlist.trackhashes = track_hashes.to_vec();
            PlaylistTable::update(&playlist).await
        } else {
            Err(anyhow::anyhow!("Playlist not found"))
        }
    }

    /// Get playlist duration
    pub async fn get_duration(playlist_id: i64) -> Result<i32> {
        let tracks = Self::get_tracks(playlist_id).await?;
        Ok(tracks.iter().map(|t| t.duration).sum())
    }

    /// Get playlist track count
    pub async fn get_track_count(playlist_id: i64) -> Result<usize> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        match playlist {
            Some(p) => Ok(p.trackhashes.len()),
            None => Ok(0),
        }
    }

    /// Duplicate playlist
    pub async fn duplicate(playlist_id: i64, new_name: Option<&str>) -> Result<i64> {
        let playlist = PlaylistTable::get_by_id(playlist_id).await?;

        match playlist {
            Some(p) => {
                let default_name = format!("{} (Copy)", p.name);
                let name = new_name.unwrap_or(&default_name);
                let mut new_playlist = p.clone();
                new_playlist.id = 0;
                new_playlist.name = name.to_string();
                PlaylistTable::insert(&new_playlist).await
            }
            None => Err(anyhow::anyhow!("Playlist not found")),
        }
    }
}
