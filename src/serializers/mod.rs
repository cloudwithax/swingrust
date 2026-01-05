//! Serializers for converting database models to API responses
//!
//! This module provides functions to serialize internal models into
//! JSON-friendly structures for API responses.

use crate::models::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackResponse {
    pub id: i64,
    pub title: String,
    pub album: String,
    pub albumhash: String,
    pub artists: Vec<String>,
    pub artisthashes: Vec<String>,
    pub duration: i32,
    pub filepath: String,
    pub trackno: i32,
    pub discno: i32,
    pub date: String,
    pub genre: String,
    pub bitrate: i32,
    pub samplerate: i32,
    pub image: Option<String>,
    pub is_favorite: bool,
    pub play_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumResponse {
    pub albumhash: String,
    pub title: String,
    pub albumartist: String,
    pub albumartisthash: String,
    pub date: String,
    pub duration: i32,
    pub trackcount: i32,
    pub image: Option<String>,
    pub color: Option<String>,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistResponse {
    pub artisthash: String,
    pub name: String,
    pub albumcount: i32,
    pub trackcount: i32,
    pub duration: i32,
    pub image: Option<String>,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistResponse {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub trackcount: i32,
    pub duration: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Track> for TrackResponse {
    fn from(track: Track) -> Self {
        let artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
        let image = if track.image.is_empty() {
            None
        } else {
            Some(track.image.clone())
        };
        let genre = track.genre();
        Self {
            id: track.id,
            title: track.title,
            album: track.album,
            albumhash: track.albumhash,
            artists,
            artisthashes: track.artisthashes,
            duration: track.duration,
            filepath: track.filepath,
            trackno: track.track,
            discno: track.disc,
            date: track.date.to_string(),
            genre,
            bitrate: track.bitrate,
            samplerate: 0, // Not stored in track model
            image,
            is_favorite: false,
            play_count: track.playcount,
        }
    }
}

impl From<Album> for AlbumResponse {
    fn from(album: Album) -> Self {
        let albumartisthash = album.artisthashes.first().cloned().unwrap_or_default();
        let image = if album.image.is_empty() {
            None
        } else {
            Some(album.image.clone())
        };
        let color = if album.color.is_empty() {
            None
        } else {
            Some(album.color.clone())
        };
        let albumartist = album.albumartist();
        Self {
            albumhash: album.albumhash,
            title: album.title,
            albumartist,
            albumartisthash,
            date: album.date.to_string(),
            duration: album.duration,
            trackcount: album.trackcount,
            image,
            color,
            is_favorite: false,
        }
    }
}

impl From<Artist> for ArtistResponse {
    fn from(artist: Artist) -> Self {
        let image = if artist.image.is_empty() {
            None
        } else {
            Some(artist.image)
        };
        Self {
            artisthash: artist.artisthash,
            name: artist.name,
            albumcount: artist.albumcount,
            trackcount: artist.trackcount,
            duration: artist.duration,
            image,
            is_favorite: false,
        }
    }
}
