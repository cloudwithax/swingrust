//! Track library functions

use std::collections::HashMap;

use crate::models::Track;
use crate::stores::TrackStore;

/// Track library functions
pub struct TracksLib;

impl TracksLib {
    /// Get all tracks
    pub fn get_all() -> Vec<Track> {
        TrackStore::get().get_all()
    }

    /// Get track by hash
    pub fn get_by_hash(hash: &str) -> Option<Track> {
        TrackStore::get().get_by_hash(hash)
    }

    /// Get tracks by hashes
    pub fn get_by_hashes(hashes: &[String]) -> Vec<Track> {
        TrackStore::get().get_by_hashes(hashes)
    }

    /// Get track by filepath
    pub fn get_by_path(path: &str) -> Option<Track> {
        TrackStore::get().get_by_path(path)
    }

    /// Get total track count
    pub fn count() -> usize {
        TrackStore::get().count()
    }

    /// Get paginated tracks
    pub fn get_paginated(page: usize, limit: usize) -> Vec<Track> {
        let tracks = TrackStore::get().get_all();
        let start = page * limit;

        if start >= tracks.len() {
            return Vec::new();
        }

        tracks.into_iter().skip(start).take(limit).collect()
    }

    /// Get random tracks
    pub fn get_random(count: usize) -> Vec<Track> {
        use rand::seq::SliceRandom;

        let tracks = TrackStore::get().get_all();
        let mut rng = rand::thread_rng();

        tracks
            .choose_multiple(&mut rng, count.min(tracks.len()))
            .cloned()
            .collect()
    }

    /// Get tracks by genre
    pub fn get_by_genre(genre: &str) -> Vec<Track> {
        let genre_lower = genre.to_lowercase();
        TrackStore::get()
            .get_all()
            .into_iter()
            .filter(|t| t.genre().to_lowercase().contains(&genre_lower))
            .collect()
    }

    /// Get all unique genres
    pub fn get_all_genres() -> Vec<String> {
        let mut genres: Vec<String> = TrackStore::get()
            .get_all()
            .iter()
            .filter(|t| !t.genre().is_empty())
            .map(|t| t.genre().clone())
            .collect();

        genres.sort();
        genres.dedup();
        genres
    }

    /// Get tracks by year
    pub fn get_by_year(year: i32) -> Vec<Track> {
        TrackStore::get()
            .get_all()
            .into_iter()
            .filter(|t| t.date == year as i64)
            .collect()
    }

    /// Get tracks added in date range
    pub fn get_recently_added(days: i64) -> Vec<Track> {
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - (days * 24 * 60 * 60);

        TrackStore::get()
            .get_all()
            .into_iter()
            .filter(|t| t.date >= cutoff)
            .collect()
    }

    /// Get total duration of all tracks
    pub fn total_duration() -> i64 {
        TrackStore::get()
            .get_all()
            .iter()
            .map(|t| t.duration as i64)
            .sum()
    }

    /// Group tracks by album
    pub fn group_by_album() -> HashMap<String, Vec<Track>> {
        let mut groups: HashMap<String, Vec<Track>> = HashMap::new();

        for track in TrackStore::get().get_all() {
            groups
                .entry(track.albumhash.clone())
                .or_insert_with(Vec::new)
                .push(track);
        }

        groups
    }

    /// Group tracks by artist
    pub fn group_by_artist() -> HashMap<String, Vec<Track>> {
        let mut groups: HashMap<String, Vec<Track>> = HashMap::new();

        for track in TrackStore::get().get_all() {
            for artist_hash in &track.artisthashes {
                groups
                    .entry(artist_hash.clone())
                    .or_insert_with(Vec::new)
                    .push(track.clone());
            }
        }

        groups
    }

    /// Search tracks
    pub fn search(query: &str, limit: usize) -> Vec<Track> {
        let query_lower = query.to_lowercase();

        TrackStore::get()
            .get_all()
            .into_iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&query_lower)
                    || t.album.to_lowercase().contains(&query_lower)
                    || t.artist().to_lowercase().contains(&query_lower)
            })
            .take(limit)
            .collect()
    }

    /// Get tracks in a folder
    pub fn get_by_folder(folder_path: &str) -> Vec<Track> {
        TrackStore::get()
            .get_all()
            .into_iter()
            .filter(|t| t.folder == folder_path)
            .collect()
    }

    /// Get recent tracks (most recently added, by last_mod)
    pub fn get_recent(limit: usize) -> Vec<Track> {
        let mut tracks = TrackStore::get().get_all();
        tracks.sort_by(|a, b| b.last_mod.cmp(&a.last_mod));
        tracks.into_iter().take(limit).collect()
    }
}
