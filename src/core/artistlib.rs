//! Artist library functions

use std::collections::HashMap;

use crate::models::{Album, Artist, GenreRef, Track};
use crate::stores::{AlbumStore, ArtistStore, TrackStore};

/// Artist library functions
pub struct ArtistLib;

impl ArtistLib {
    /// Get all artists
    pub fn get_all() -> Vec<Artist> {
        ArtistStore::get().get_all()
    }

    /// Get artist by hash
    pub fn get_by_hash(hash: &str) -> Option<Artist> {
        ArtistStore::get().get_by_hash(hash)
    }

    /// Get artist by name
    pub fn get_by_name(name: &str) -> Option<Artist> {
        ArtistStore::get().get_by_name(name)
    }

    /// Get artist tracks
    pub fn get_tracks(artist_hash: &str) -> Vec<Track> {
        TrackStore::get().get_by_artist(artist_hash)
    }

    /// Get artist albums
    pub fn get_albums(artist_hash: &str) -> Vec<Album> {
        AlbumStore::get().get_by_artist(artist_hash)
    }

    /// Get artist albums where they are the album artist
    pub fn get_main_albums(artist_hash: &str) -> Vec<Album> {
        AlbumStore::get()
            .get_by_artist(artist_hash)
            .into_iter()
            .filter(|a| a.albumartists.iter().any(|aa| aa.artisthash == artist_hash))
            .collect()
    }

    /// Get albums where artist appears but isn't the main artist
    pub fn get_appearances(artist_hash: &str) -> Vec<Album> {
        AlbumStore::get()
            .get_by_artist(artist_hash)
            .into_iter()
            .filter(|a| !a.albumartists.iter().any(|aa| aa.artisthash == artist_hash))
            .collect()
    }

    /// Build artists from tracks
    pub fn build_artists(tracks: &[Track]) -> Vec<Artist> {
        let mut artist_map: HashMap<String, Artist> = HashMap::new();
        // track unique genres per artist by genrehash to avoid duplicates
        let mut artist_genres: HashMap<String, HashMap<String, GenreRef>> = HashMap::new();

        for track in tracks {
            // Add track artists
            for artist_ref in &track.artists {
                let hash = &artist_ref.artisthash;
                let name = &artist_ref.name;

                artist_map
                    .entry(hash.clone())
                    .and_modify(|artist| {
                        artist.trackcount += 1;
                    })
                    .or_insert_with(|| {
                        let mut artist = Artist::new(name.to_string(), hash.clone());
                        artist.trackcount = 1;
                        artist.created_date = track.date;
                        artist
                    });

                // collect genres from this track for this artist
                let genre_map = artist_genres.entry(hash.clone()).or_default();
                for genre in &track.genres {
                    genre_map
                        .entry(genre.genrehash.clone())
                        .or_insert_with(|| genre.clone());
                }
            }

            // also add album artists if different from track artists
            for artist_ref in &track.albumartists {
                if !track.artisthashes.contains(&artist_ref.artisthash) {
                    artist_map
                        .entry(artist_ref.artisthash.clone())
                        .or_insert_with(|| {
                            let mut artist =
                                Artist::new(artist_ref.name.clone(), artist_ref.artisthash.clone());
                            artist.created_date = track.date;
                            artist
                        });

                    // collect genres for album artists too
                    let genre_map = artist_genres.entry(artist_ref.artisthash.clone()).or_default();
                    for genre in &track.genres {
                        genre_map
                            .entry(genre.genrehash.clone())
                            .or_insert_with(|| genre.clone());
                    }
                }
            }
        }

        // calculate album counts
        let album_store = AlbumStore::get();
        for artist in artist_map.values_mut() {
            artist.albumcount = album_store.get_by_artist(&artist.artisthash).len() as i32;
        }

        // calculate duration
        let track_store = TrackStore::get();
        for artist in artist_map.values_mut() {
            artist.duration = track_store
                .get_by_artist(&artist.artisthash)
                .iter()
                .map(|t| t.duration)
                .sum();
        }

        // set genres on each artist
        for artist in artist_map.values_mut() {
            if let Some(genre_map) = artist_genres.get(&artist.artisthash) {
                artist.genres = genre_map.values().cloned().collect();
                // sort genres alphabetically by name for consistent ordering
                artist.genres.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                artist.compute_genrehashes();
            }
        }

        artist_map.into_values().collect()
    }

    /// Collect artist genres from tracks
    pub fn collect_genres(artist_hash: &str) -> Vec<String> {
        let tracks = Self::get_tracks(artist_hash);
        let mut genres: Vec<String> = tracks
            .iter()
            .filter(|t| !t.genre().is_empty())
            .map(|t| t.genre().clone())
            .collect();

        genres.sort();
        genres.dedup();
        genres
    }

    /// Get total artist count
    pub fn count() -> usize {
        ArtistStore::get().count()
    }

    /// Get paginated artists
    pub fn get_paginated(page: usize, limit: usize) -> Vec<Artist> {
        let artists = ArtistStore::get().get_all();
        let start = page * limit;

        if start >= artists.len() {
            return Vec::new();
        }

        artists.into_iter().skip(start).take(limit).collect()
    }

    /// Search artists
    pub fn search(query: &str, limit: usize) -> Vec<Artist> {
        ArtistStore::get().search_by_name(query, limit)
    }
}
