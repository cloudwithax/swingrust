//! Album library functions

use std::collections::HashMap;

use crate::models::{Album, Track};
use crate::stores::{AlbumStore, TrackStore};

/// Album library functions
pub struct AlbumLib;

impl AlbumLib {
    /// Get all albums
    pub fn get_all() -> Vec<Album> {
        AlbumStore::get().get_all()
    }

    /// Get album by hash
    pub fn get_by_hash(hash: &str) -> Option<Album> {
        AlbumStore::get().get_by_hash(hash)
    }

    /// Get albums by artist hash
    pub fn get_by_artist(artist_hash: &str) -> Vec<Album> {
        AlbumStore::get().get_by_artist(artist_hash)
    }

    /// Get album tracks
    pub fn get_tracks(album_hash: &str) -> Vec<Track> {
        let mut tracks = TrackStore::get().get_by_album(album_hash);

        // Sort by disc and track number
        tracks.sort_by(|a, b| {
            let disc_cmp = a.disc.cmp(&b.disc);
            if disc_cmp != std::cmp::Ordering::Equal {
                disc_cmp
            } else {
                a.track.cmp(&b.track)
            }
        });

        tracks
    }

    /// Build albums from tracks
    pub fn build_albums(tracks: &[Track]) -> Vec<Album> {
        let mut album_map: HashMap<String, Album> = HashMap::new();

        for track in tracks {
            let hash = &track.albumhash;

            album_map
                .entry(hash.clone())
                .and_modify(|album| {
                    album.trackcount += 1;
                    album.duration += track.duration;

                    // Update earliest release date
                    if track.date < album.date {
                        album.date = track.date;
                    }

                    // Track earliest created date
                    if track.date < album.created_date {
                        album.created_date = track.date;
                    }
                })
                .or_insert_with(|| {
                    let mut album = Album::new(hash.clone(), track.og_album.clone());
                    album.albumartists = track.albumartists.clone();
                    album.artisthashes = track.artisthashes.clone();
                    album.date = track.date;
                    album.duration = track.duration;
                    album.trackcount = 1;
                    album.created_date = track.date;
                    album.genres = track.genres.clone();
                    album.genrehashes = track.genrehashes.clone();
                    // Set pathhash from the track folder and generate image path
                    let pathhash = track.folderhash();
                    album.pathhash = pathhash.clone();
                    album.image = format!("{}.webp?pathhash={}", album.albumhash, pathhash);
                    album
                });
        }

        album_map.into_values().collect()
    }

    /// Collect album genres from tracks
    pub fn collect_genres(album_hash: &str) -> Vec<String> {
        let tracks = Self::get_tracks(album_hash);
        let mut genres: Vec<String> = tracks
            .iter()
            .filter(|t| !t.genre().is_empty())
            .map(|t| t.genre().clone())
            .collect();

        genres.sort();
        genres.dedup();
        genres
    }

    /// Get album versions (same base title, different versions)
    pub fn get_versions(album: &Album) -> Vec<Album> {
        let store = AlbumStore::get();
        let all_albums = store.get_all();

        // Get base title (without version info)
        let base_title: String = if album.base_title.is_empty() {
            album.title.to_lowercase()
        } else {
            album.base_title.to_lowercase()
        };

        all_albums
            .into_iter()
            .filter(|a| {
                a.albumhash != album.albumhash
                    && a.albumartist().to_lowercase() == album.albumartist().to_lowercase()
                    && if a.base_title.is_empty() {
                        a.title.to_lowercase() == base_title
                    } else {
                        a.base_title.to_lowercase() == base_title
                    }
            })
            .collect()
    }

    /// Get total album count
    pub fn count() -> usize {
        AlbumStore::get().count()
    }

    /// Get paginated albums
    pub fn get_paginated(page: usize, limit: usize) -> Vec<Album> {
        let albums = AlbumStore::get().get_all();
        let start = page * limit;

        if start >= albums.len() {
            return Vec::new();
        }

        albums.into_iter().skip(start).take(limit).collect()
    }
}
