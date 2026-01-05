//! Search functionality for tracks, albums, artists

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::models::{Album, Artist, Track};
use crate::stores::{AlbumStore, ArtistStore, TrackStore};

/// Search result item
#[derive(Debug, Clone)]
pub struct SearchResult<T> {
    pub item: T,
    pub score: f64,
}

/// Search library
pub struct SearchLib;

impl SearchLib {
    /// Search tracks by query
    pub fn search_tracks(query: &str, limit: usize) -> Vec<SearchResult<Track>> {
        let store = TrackStore::get();
        let tracks = store.get_all();

        Self::fuzzy_search(&tracks, query, |t| &t.title, limit)
    }

    /// Search albums by query
    pub fn search_albums(query: &str, limit: usize) -> Vec<SearchResult<Album>> {
        let store = AlbumStore::get();
        let albums = store.get_all();

        Self::fuzzy_search(&albums, query, |a| &a.title, limit)
    }

    /// Search artists by query
    pub fn search_artists(query: &str, limit: usize) -> Vec<SearchResult<Artist>> {
        let store = ArtistStore::get();
        let artists = store.get_all();

        Self::fuzzy_search(&artists, query, |a| &a.name, limit)
    }

    /// Combined search across all types
    pub fn search_all(
        query: &str,
        tracks_limit: usize,
        albums_limit: usize,
        artists_limit: usize,
    ) -> (
        Vec<SearchResult<Track>>,
        Vec<SearchResult<Album>>,
        Vec<SearchResult<Artist>>,
    ) {
        let tracks = Self::search_tracks(query, tracks_limit);
        let albums = Self::search_albums(query, albums_limit);
        let artists = Self::search_artists(query, artists_limit);

        (tracks, albums, artists)
    }

    /// Fuzzy search implementation
    fn fuzzy_search<T: Clone>(
        items: &[T],
        query: &str,
        get_name: impl Fn(&T) -> &str,
        limit: usize,
    ) -> Vec<SearchResult<T>> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<SearchResult<T>> = items
            .iter()
            .filter_map(|item| {
                let name = get_name(item).to_lowercase();
                let score = Self::calculate_score(&name, &query_lower, &query_words);

                if score > 0.0 {
                    Some(SearchResult {
                        item: item.clone(),
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        results.truncate(limit);
        results
    }

    /// Calculate search score
    fn calculate_score(text: &str, query: &str, query_words: &[&str]) -> f64 {
        let mut score = 0.0;

        // Exact match gets highest score
        if text == query {
            return 1000.0;
        }

        // Starts with query
        if text.starts_with(query) {
            score += 100.0;
        }

        // Contains query
        if text.contains(query) {
            score += 50.0;
        }

        // Word matches
        let text_words: Vec<&str> = text.split_whitespace().collect();
        for query_word in query_words {
            for text_word in &text_words {
                if *text_word == *query_word {
                    score += 30.0;
                } else if text_word.starts_with(query_word) {
                    score += 20.0;
                } else if text_word.contains(query_word) {
                    score += 10.0;
                }
            }
        }

        // Levenshtein distance for fuzzy matching
        if score == 0.0 {
            let distance = Self::levenshtein(text, query);
            let max_len = text.len().max(query.len()) as f64;
            if max_len > 0.0 {
                let similarity = 1.0 - (distance as f64 / max_len);
                if similarity > 0.5 {
                    score = similarity * 20.0;
                }
            }
        }

        score
    }

    /// Calculate Levenshtein distance
    fn levenshtein(s1: &str, s2: &str) -> usize {
        let m = s1.len();
        let n = s2.len();

        if m == 0 {
            return n;
        }
        if n == 0 {
            return m;
        }

        let mut dp = vec![vec![0; n + 1]; m + 1];

        for i in 0..=m {
            dp[i][0] = i;
        }
        for j in 0..=n {
            dp[0][j] = j;
        }

        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();

        for i in 1..=m {
            for j in 1..=n {
                let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                    0
                } else {
                    1
                };
                dp[i][j] = (dp[i - 1][j] + 1)
                    .min(dp[i][j - 1] + 1)
                    .min(dp[i - 1][j - 1] + cost);
            }
        }

        dp[m][n]
    }

    /// Top results by play count
    pub fn top_tracks(limit: usize, play_counts: &HashMap<String, i32>) -> Vec<Track> {
        let store = TrackStore::get();
        let mut tracks_with_plays: Vec<_> = store
            .get_all()
            .into_iter()
            .map(|t| {
                let plays = play_counts.get(&t.trackhash).copied().unwrap_or(0);
                (t, plays)
            })
            .filter(|(_, plays)| *plays > 0)
            .collect();

        tracks_with_plays.sort_by(|a, b| b.1.cmp(&a.1));

        tracks_with_plays
            .into_iter()
            .take(limit)
            .map(|(t, _)| t)
            .collect()
    }
}
