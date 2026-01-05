//! Recipe system for generating mixes

use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};

use crate::db::tables::ScrobbleTable;
use crate::models::Track;
use crate::stores::{ArtistStore, TrackStore};
use crate::utils::dates::get_timestamp_days_ago;

/// Mix/Recipe result
#[derive(Debug, Clone)]
pub struct Mix {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tracks: Vec<Track>,
    pub image: Option<String>,
}

/// Artist stats for top artists
#[derive(Debug, Clone)]
pub struct ArtistStats {
    pub artisthash: String,
    pub name: String,
    pub image: String,
    pub play_count: i32,
    pub duration: i64,
}

/// Recipe generators
pub struct Recipes;

impl Recipes {
    /// Get recently played tracks
    pub async fn recently_played(limit: usize) -> Vec<Track> {
        let scrobbles = ScrobbleTable::get_paginated_default(0, limit as i64)
            .await
            .unwrap_or_default();

        let mut tracks = Vec::new();
        let mut seen = HashSet::new();

        for scrobble in scrobbles {
            if seen.insert(scrobble.trackhash.clone()) {
                if let Some(track) = TrackStore::get().get_by_hash(&scrobble.trackhash) {
                    tracks.push(track);
                }
            }
        }

        tracks
    }

    /// Get recently added tracks
    pub fn recently_added(limit: usize) -> Vec<Track> {
        let mut tracks = TrackStore::get().get_all();
        tracks.sort_by(|a, b| b.last_mod.cmp(&a.last_mod));
        tracks.into_iter().take(limit).collect()
    }

    /// Get top streamed tracks
    pub async fn top_streamed(days: i64, limit: usize) -> Vec<Track> {
        let start = get_timestamp_days_ago(days);
        let end = chrono::Utc::now().timestamp();

        let scrobbles = ScrobbleTable::get_by_time_range(start, end)
            .await
            .unwrap_or_default();

        let mut play_counts: HashMap<String, i32> = HashMap::new();
        for scrobble in scrobbles {
            *play_counts.entry(scrobble.trackhash).or_insert(0) += 1;
        }

        let mut sorted: Vec<_> = play_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        sorted
            .into_iter()
            .take(limit)
            .filter_map(|(hash, _)| TrackStore::get().get_by_hash(&hash))
            .collect()
    }

    /// "Because you listened to" mix
    pub async fn because_you_listened_to(artist_hash: &str, limit: usize) -> Option<Mix> {
        let artist = ArtistStore::get().get_by_hash(artist_hash)?;

        // Get tracks from this artist
        let artist_tracks = TrackStore::get().get_by_artist(artist_hash);

        if artist_tracks.is_empty() {
            return None;
        }

        // Get similar genre tracks
        let mut genre_hashes: HashSet<String> = HashSet::new();
        for track in &artist_tracks {
            for hash in &track.genrehashes {
                genre_hashes.insert(hash.clone());
            }
        }

        let all_tracks = TrackStore::get().get_all();
        let mut similar_tracks: Vec<Track> = all_tracks
            .into_iter()
            .filter(|t| {
                !t.artisthashes.contains(&artist_hash.to_string())
                    && t.genrehashes.iter().any(|g| genre_hashes.contains(g))
            })
            .collect();

        similar_tracks.shuffle(&mut rand::thread_rng());
        similar_tracks.truncate(limit);

        Some(Mix {
            id: format!("because-{}", artist_hash),
            name: format!("Because you listened to {}", artist.name),
            description: format!("Tracks similar to {}", artist.name),
            tracks: similar_tracks,
            image: Some(artist.image.clone()),
        })
    }

    /// Artist mix - deep dive into an artist
    pub fn artist_mix(artist_hash: &str, limit: usize) -> Option<Mix> {
        let artist = ArtistStore::get().get_by_hash(artist_hash)?;

        let mut tracks = TrackStore::get().get_by_artist(artist_hash);
        tracks.shuffle(&mut rand::thread_rng());
        tracks.truncate(limit);

        if tracks.is_empty() {
            return None;
        }

        Some(Mix {
            id: format!("artist-mix-{}", artist_hash),
            name: format!("{} Mix", artist.name),
            description: format!("A mix of tracks by {}", artist.name),
            tracks,
            image: Some(artist.image.clone()),
        })
    }

    /// Genre mix
    pub fn genre_mix(genre: &str, limit: usize) -> Option<Mix> {
        let genre_lower = genre.to_lowercase();
        let genre_hash = crate::utils::hashing::create_hash(&[genre], true);

        let all_tracks = TrackStore::get().get_all();
        let mut tracks: Vec<Track> = all_tracks
            .into_iter()
            .filter(|t| {
                t.genrehashes.contains(&genre_hash)
                    || t.genre().to_lowercase().contains(&genre_lower)
            })
            .collect();

        if tracks.is_empty() {
            return None;
        }

        tracks.shuffle(&mut rand::thread_rng());
        tracks.truncate(limit);

        Some(Mix {
            id: format!("genre-{}", genre_hash),
            name: format!("{} Mix", genre),
            description: format!("Tracks in the {} genre", genre),
            tracks,
            image: None,
        })
    }

    /// Decade mix
    pub fn decade_mix(decade: i32, limit: usize) -> Option<Mix> {
        let start_year = decade;
        let end_year = decade + 9;

        let all_tracks = TrackStore::get().get_all();
        let mut tracks: Vec<Track> = all_tracks
            .into_iter()
            .filter(|t| {
                if t.date == 0 {
                    return false;
                }
                let year = chrono::DateTime::from_timestamp(t.date, 0)
                    .map(|dt| dt.format("%Y").to_string().parse::<i32>().unwrap_or(0))
                    .unwrap_or(0);
                year >= start_year && year <= end_year
            })
            .collect();

        if tracks.is_empty() {
            return None;
        }

        tracks.shuffle(&mut rand::thread_rng());
        tracks.truncate(limit);

        Some(Mix {
            id: format!("decade-{}", decade),
            name: format!("{}s Mix", decade),
            description: format!("Tracks from the {}s", decade),
            tracks,
            image: None,
        })
    }

    /// Random mix
    pub fn random_mix(limit: usize) -> Mix {
        let mut tracks = TrackStore::get().get_all();
        tracks.shuffle(&mut rand::thread_rng());
        tracks.truncate(limit);

        Mix {
            id: "random".to_string(),
            name: "Random Mix".to_string(),
            description: "A random selection of tracks".to_string(),
            tracks,
            image: None,
        }
    }

    /// Get all available mixes for homepage
    pub async fn get_homepage_mixes() -> Vec<Mix> {
        let mut mixes = Vec::new();

        // Recently played
        let recent = Self::recently_played(20).await;
        if !recent.is_empty() {
            mixes.push(Mix {
                id: "recently-played".to_string(),
                name: "Recently Played".to_string(),
                description: "Your recently played tracks".to_string(),
                tracks: recent,
                image: None,
            });
        }

        // Recently added
        let added = Self::recently_added(20);
        if !added.is_empty() {
            mixes.push(Mix {
                id: "recently-added".to_string(),
                name: "Recently Added".to_string(),
                description: "Newly added to your library".to_string(),
                tracks: added,
                image: None,
            });
        }

        // Top streamed
        let top = Self::top_streamed(30, 20).await;
        if !top.is_empty() {
            mixes.push(Mix {
                id: "top-streamed".to_string(),
                name: "Top Streamed".to_string(),
                description: "Your most played tracks this month".to_string(),
                tracks: top,
                image: None,
            });
        }

        mixes
    }

    /// Get top artists for a time period (days)
    pub async fn top_artists_in_period(days: i64, limit: usize, user_id: i64) -> Vec<ArtistStats> {
        let start = get_timestamp_days_ago(days);
        let end = chrono::Utc::now().timestamp();

        let scrobbles = ScrobbleTable::get_in_range(user_id, start, end)
            .await
            .unwrap_or_default();

        // aggregate by artist
        let mut artist_stats: HashMap<String, (i32, i64)> = HashMap::new();
        let track_store = TrackStore::get();

        for scrobble in scrobbles {
            if let Some(track) = track_store.get_by_hash(&scrobble.trackhash) {
                for artisthash in &track.artisthashes {
                    let entry = artist_stats.entry(artisthash.clone()).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += scrobble.duration as i64;
                }
            }
        }

        // convert to stats
        let mut stats: Vec<ArtistStats> = artist_stats
            .into_iter()
            .filter_map(|(artisthash, (play_count, duration))| {
                let artist = ArtistStore::get().get_by_hash(&artisthash)?;
                Some(ArtistStats {
                    artisthash,
                    name: artist.name.clone(),
                    image: artist.image.clone(),
                    play_count,
                    duration,
                })
            })
            .collect();

        stats.sort_by(|a, b| b.play_count.cmp(&a.play_count));
        stats.truncate(limit);
        stats
    }

    /// Get top artists this week
    pub async fn top_artists_weekly(limit: usize, user_id: i64) -> Vec<ArtistStats> {
        Self::top_artists_in_period(7, limit, user_id).await
    }

    /// Get top artists this month
    pub async fn top_artists_monthly(limit: usize, user_id: i64) -> Vec<ArtistStats> {
        Self::top_artists_in_period(30, limit, user_id).await
    }

    /// Generate artist mixes for homepage based on listening history
    pub async fn generate_artist_mixes(limit: usize, user_id: i64) -> Vec<crate::models::Mix> {
        let mut mixes = Vec::new();

        // get top artists from recent listening
        let top = Self::top_artists_in_period(30, limit * 2, user_id).await;

        for stats in top.into_iter().take(limit) {
            if let Some(artist) = ArtistStore::get().get_by_hash(&stats.artisthash) {
                let mut tracks = TrackStore::get().get_by_artist(&stats.artisthash);
                if tracks.is_empty() {
                    continue;
                }

                tracks.shuffle(&mut rand::thread_rng());
                tracks.truncate(40);

                // build mix
                let mix = crate::models::Mix::new(
                    format!("a{}", stats.artisthash),
                    format!("{} Radio", artist.name),
                    Self::build_mix_description(&tracks, &stats.artisthash),
                    tracks.iter().map(|t| t.trackhash.clone()).collect(),
                    stats.artisthash.clone(),
                    0,
                );

                mixes.push(mix);
            }
        }

        mixes
    }

    /// Generate daily mixes (spotify-style) based on listening history
    /// starts working with just 1 day of activity
    pub async fn generate_daily_mixes(max_mixes: usize, user_id: i64) -> Vec<crate::models::Mix> {
        let mut mixes = Vec::new();

        // get scrobbles from the last 30 days (works with minimal data)
        let start = get_timestamp_days_ago(30);
        let end = chrono::Utc::now().timestamp();

        let scrobbles = ScrobbleTable::get_in_range(user_id, start, end)
            .await
            .unwrap_or_default();

        if scrobbles.is_empty() {
            return mixes;
        }

        // build artist play counts
        let mut artist_play_counts: HashMap<String, i32> = HashMap::new();
        let track_store = TrackStore::get();

        for scrobble in &scrobbles {
            if let Some(track) = track_store.get_by_hash(&scrobble.trackhash) {
                // count primary artist (first in list)
                if let Some(primary_artist) = track.artisthashes.first() {
                    *artist_play_counts.entry(primary_artist.clone()).or_insert(0) += 1;
                }
            }
        }

        // sort artists by play count
        let mut sorted_artists: Vec<_> = artist_play_counts.into_iter().collect();
        sorted_artists.sort_by(|a, b| b.1.cmp(&a.1));

        // take top artists as seeds for daily mixes
        let seed_artists: Vec<String> = sorted_artists
            .into_iter()
            .take(max_mixes * 2)
            .map(|(hash, _)| hash)
            .collect();

        let mut used_artists: HashSet<String> = HashSet::new();
        let mut mix_number = 1;

        for seed_artisthash in seed_artists {
            if mixes.len() >= max_mixes {
                break;
            }

            // skip if we already used this artist as a seed
            if used_artists.contains(&seed_artisthash) {
                continue;
            }
            used_artists.insert(seed_artisthash.clone());

            let artist = match ArtistStore::get().get_by_hash(&seed_artisthash) {
                Some(a) => a,
                None => continue,
            };

            // get all tracks by seed artist
            let mut seed_tracks = track_store.get_by_artist(&seed_artisthash);
            if seed_tracks.is_empty() {
                continue;
            }

            // collect genres from seed artist tracks
            let mut genre_hashes: HashSet<String> = HashSet::new();
            for track in &seed_tracks {
                for hash in &track.genrehashes {
                    genre_hashes.insert(hash.clone());
                }
            }

            // find related tracks (same genres, different artist)
            let all_tracks = track_store.get_all();
            let mut related_tracks: Vec<_> = all_tracks
                .into_iter()
                .filter(|t| {
                    !t.artisthashes.contains(&seed_artisthash)
                        && t.genrehashes.iter().any(|g| genre_hashes.contains(g))
                })
                .collect();

            // shuffle both pools
            seed_tracks.shuffle(&mut rand::thread_rng());
            related_tracks.shuffle(&mut rand::thread_rng());

            // compose mix: ~60% seed artist, ~40% related
            let seed_count = 15.min(seed_tracks.len());
            let related_count = 10.min(related_tracks.len());

            let mut mix_tracks: Vec<crate::models::Track> = Vec::new();
            mix_tracks.extend(seed_tracks.into_iter().take(seed_count));
            mix_tracks.extend(related_tracks.into_iter().take(related_count));

            // shuffle the final mix
            mix_tracks.shuffle(&mut rand::thread_rng());

            // ensure we have at least a few tracks
            if mix_tracks.len() < 5 {
                continue;
            }

            // limit to 25 tracks
            mix_tracks.truncate(25);

            // collect images from first few tracks
            let images: Vec<String> = mix_tracks
                .iter()
                .take(4)
                .map(|t| t.image.clone())
                .collect();

            let mix = crate::models::Mix::new(
                format!("d{}", mix_number),
                format!("Daily Mix {}", mix_number),
                Self::build_daily_mix_description(&mix_tracks, &seed_artisthash, &artist.name),
                mix_tracks.iter().map(|t| t.trackhash.clone()).collect(),
                seed_artisthash.clone(),
                0,
            );

            // add images to the mix
            let mut mix_with_images = mix;
            mix_with_images.images = images;

            mixes.push(mix_with_images);
            mix_number += 1;
        }

        mixes
    }

    /// Build description for daily mix showing featured artists
    fn build_daily_mix_description(tracks: &[Track], seed_artisthash: &str, seed_name: &str) -> String {
        let mut featured: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        seen.insert(seed_artisthash.to_string());

        for track in tracks {
            if featured.len() >= 3 {
                break;
            }
            if let Some(first_artist) = track.artisthashes.first() {
                if !seen.contains(first_artist) {
                    if let Some(artist) = ArtistStore::get().get_by_hash(first_artist) {
                        featured.push(artist.name.clone());
                        seen.insert(first_artist.clone());
                    }
                }
            }
        }

        if featured.len() >= 2 {
            format!("{}, {} and more", seed_name, featured.join(", "))
        } else if !featured.is_empty() {
            format!("{} and {}", seed_name, featured[0])
        } else {
            format!("Featuring {}", seed_name)
        }
    }

    /// Build mix description from featured artists
    fn build_mix_description(tracks: &[Track], main_artisthash: &str) -> String {
        let mut featured: Vec<String> = Vec::new();
        let mut seen = HashSet::new();

        for track in tracks {
            if featured.len() >= 4 {
                break;
            }
            if let Some(first_artist) = track.artisthashes.first() {
                if first_artist != main_artisthash && !seen.contains(first_artist) {
                    if let Some(artist) = ArtistStore::get().get_by_hash(first_artist) {
                        featured.push(artist.name.clone());
                        seen.insert(first_artist.clone());
                    }
                }
            }
        }

        if featured.len() >= 4 {
            format!("Featuring {}, {} and more", featured[..2].join(", "), featured[2])
        } else if !featured.is_empty() {
            format!("Featuring {}", featured.join(", "))
        } else {
            "A personalized mix".to_string()
        }
    }

    /// Get recently played items with various source types (tracks, albums, artists, folders, playlists, mixes)
    pub async fn recently_played_items(limit: usize, user_id: i64) -> Vec<RecentlyPlayedItem> {
        let scrobbles = ScrobbleTable::get_paginated(user_id, 0, (limit as i64 * 5).max(100))
            .await
            .unwrap_or_default();

        let mut items = Vec::new();
        let mut seen_sources = HashSet::new();
        let track_store = TrackStore::get();

        for scrobble in scrobbles {
            if items.len() >= limit {
                break;
            }

            // dedupe by source to avoid showing the same album/artist multiple times
            let source_key = if scrobble.source.is_empty() || scrobble.source == "unknown" {
                scrobble.trackhash.clone()
            } else {
                scrobble.source.clone()
            };

            if seen_sources.contains(&source_key) {
                continue;
            }
            seen_sources.insert(source_key);

            // parse source type
            let item = Self::parse_scrobble_to_item(&scrobble, &track_store);
            if let Some(item) = item {
                items.push(item);
            }
        }

        items
    }

    fn parse_scrobble_to_item(scrobble: &crate::models::TrackLog, track_store: &std::sync::Arc<TrackStore>) -> Option<RecentlyPlayedItem> {
        let source = &scrobble.source;

        // parse source format: "prefix:id" or special values
        if let Some((prefix, id)) = source.split_once(':') {
            match prefix {
                "al" => {
                    // album source
                    if crate::stores::AlbumStore::get().get_by_hash(id).is_some() {
                        return Some(RecentlyPlayedItem {
                            item_type: "album".to_string(),
                            hash: id.to_string(),
                            timestamp: scrobble.timestamp,
                            help_text: Some("album".to_string()),
                        });
                    }
                }
                "ar" => {
                    // artist source
                    if crate::stores::ArtistStore::get().get_by_hash(id).is_some() {
                        return Some(RecentlyPlayedItem {
                            item_type: "artist".to_string(),
                            hash: id.to_string(),
                            timestamp: scrobble.timestamp,
                            help_text: Some("artist".to_string()),
                        });
                    }
                }
                "fo" => {
                    // folder source
                    return Some(RecentlyPlayedItem {
                        item_type: "folder".to_string(),
                        hash: id.to_string(),
                        timestamp: scrobble.timestamp,
                        help_text: Some("folder".to_string()),
                    });
                }
                "pl" => {
                    // playlist source
                    return Some(RecentlyPlayedItem {
                        item_type: "playlist".to_string(),
                        hash: id.to_string(),
                        timestamp: scrobble.timestamp,
                        help_text: Some("playlist".to_string()),
                    });
                }
                "mix" => {
                    // mix source
                    return Some(RecentlyPlayedItem {
                        item_type: "mix".to_string(),
                        hash: id.to_string(),
                        timestamp: scrobble.timestamp,
                        help_text: Some("mix".to_string()),
                    });
                }
                _ => {}
            }
        }

        if source == "favorite" {
            return Some(RecentlyPlayedItem {
                item_type: "favorite".to_string(),
                hash: "".to_string(),
                timestamp: scrobble.timestamp,
                help_text: Some("favorite".to_string()),
            });
        }

        // default to track
        if track_store.get_by_hash(&scrobble.trackhash).is_some() {
            return Some(RecentlyPlayedItem {
                item_type: "track".to_string(),
                hash: scrobble.trackhash.clone(),
                timestamp: scrobble.timestamp,
                help_text: Some("track".to_string()),
            });
        }

        None
    }
}

/// Recently played item (various types)
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecentlyPlayedItem {
    pub item_type: String,
    pub hash: String,
    pub timestamp: i64,
    pub help_text: Option<String>,
}
