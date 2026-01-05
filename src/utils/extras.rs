//! extra metadata helpers for favorites and scrobbles

use serde_json::{json, Value};

use crate::stores::{AlbumStore, ArtistStore, TrackStore};

/// build extra info for track, album, or artist
pub fn get_extra_info(hash: &str, item_type: &str) -> Value {
    match item_type {
        "track" => {
            if let Some(track) = TrackStore::get().get_by_hash(hash) {
                let artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
                json!({"filepath": track.filepath, "title": track.title, "artists": artists, "album": track.albumhash})
            } else {
                json!({})
            }
        }
        "album" => {
            if let Some(mut album) = AlbumStore::get().get_by_hash(hash) {
                let artists: Vec<String> = album.albumartists.drain(..).map(|a| a.name).collect();
                json!({"albumartists": artists, "title": album.title})
            } else {
                json!({})
            }
        }
        "artist" => {
            if let Some(artist) = ArtistStore::get().get_by_hash(hash) {
                json!({"name": artist.name})
            } else {
                json!({})
            }
        }
        _ => json!({}),
    }
}
