//! Search API routes
//!
//! implements search endpoints matching upstream swingmusic api

use actix_web::{get, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::core::SearchLib;
use crate::models::{Album, Artist, Track};
use crate::stores::{AlbumStore, TrackStore};

const SEARCH_COUNT: usize = 30;

/// search query parameters for get top results
#[derive(Debug, Deserialize)]
pub struct TopResultsQuery {
    pub q: String,
    #[serde(default = "default_top_limit")]
    pub limit: usize,
}

fn default_top_limit() -> usize {
    6
}

/// search query parameters for load more results
#[derive(Debug, Deserialize)]
pub struct SearchLoadMoreQuery {
    pub q: String,
    pub itemtype: String,
    #[serde(default)]
    pub start: usize,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    SEARCH_COUNT
}

/// serialized track for search results
#[derive(Debug, Clone, Serialize)]
pub struct TrackSearchResult {
    pub trackhash: String,
    pub title: String,
    pub album: String,
    pub albumhash: String,
    pub artists: Vec<ArtistRefResult>,
    pub duration: i32,
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub og_title: Option<String>,
    pub filepath: String,
    pub bitrate: i32,
    #[serde(default)]
    pub is_favorite: bool,
}

impl From<Track> for TrackSearchResult {
    fn from(track: Track) -> Self {
        let image = if track.image.is_empty() {
            format!("{}.webp", track.albumhash)
        } else {
            track.image
        };
        Self {
            trackhash: track.trackhash,
            title: track.title.clone(),
            album: track.album,
            albumhash: track.albumhash,
            artists: track.artists.into_iter().map(|a| ArtistRefResult {
                name: a.name,
                artisthash: a.artisthash,
            }).collect(),
            duration: track.duration,
            image,
            og_title: if track.og_title.is_empty() { None } else { Some(track.og_title) },
            filepath: track.filepath,
            bitrate: track.bitrate,
            is_favorite: false,
        }
    }
}

/// serialized album for search results (card format)
#[derive(Debug, Clone, Serialize)]
pub struct AlbumSearchResult {
    pub albumhash: String,
    pub title: String,
    pub albumartists: Vec<ArtistRefResult>,
    pub image: String,
    pub color: String,
    #[serde(rename = "type")]
    pub album_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
}

impl From<Album> for AlbumSearchResult {
    fn from(album: Album) -> Self {
        let image = if album.image.is_empty() {
            format!("{}.webp", album.albumhash)
        } else {
            album.image
        };
        Self {
            albumhash: album.albumhash,
            title: album.title,
            albumartists: album.albumartists.into_iter().map(|a| ArtistRefResult {
                name: a.name,
                artisthash: a.artisthash,
            }).collect(),
            image,
            color: album.color,
            album_type: album.album_type.to_string(),
            help_text: if album.help_text.is_empty() { None } else { Some(album.help_text) },
            is_favorite: false,
        }
    }
}

/// serialized artist for search results (card format)
#[derive(Debug, Clone, Serialize)]
pub struct ArtistSearchResult {
    pub artisthash: String,
    pub name: String,
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub albumcount: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trackcount: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
}

impl From<Artist> for ArtistSearchResult {
    fn from(artist: Artist) -> Self {
        let image = if artist.image.is_empty() {
            format!("{}.webp", artist.artisthash)
        } else {
            artist.image
        };
        Self {
            artisthash: artist.artisthash,
            name: artist.name,
            image,
            albumcount: Some(artist.albumcount),
            trackcount: Some(artist.trackcount),
            help_text: if artist.help_text.is_empty() { None } else { Some(artist.help_text) },
            is_favorite: false,
        }
    }
}

/// artist reference in results
#[derive(Debug, Clone, Serialize)]
pub struct ArtistRefResult {
    pub name: String,
    pub artisthash: String,
}

/// top result type discriminator
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum TopResult {
    #[serde(rename = "track")]
    Track(TrackSearchResult),
    #[serde(rename = "album")]
    Album(AlbumSearchResult),
    #[serde(rename = "artist")]
    Artist(ArtistSearchResult),
}

/// top results response
#[derive(Debug, Clone, Serialize)]
pub struct TopResultsResponse {
    pub top_result: serde_json::Value,
    pub tracks: Vec<TrackSearchResult>,
    pub artists: Vec<ArtistSearchResult>,
    pub albums: Vec<AlbumSearchResult>,
}

/// search load more response
#[derive(Debug, Clone, Serialize)]
pub struct SearchLoadMoreResponse<T> {
    pub results: Vec<T>,
    pub more: bool,
}

/// internal search result with score for sorting
#[derive(Debug, Clone)]
enum ScoredItem {
    Track(Track, f64),
    Album(Album, f64),
    Artist(Artist, f64),
}

impl ScoredItem {
    fn score(&self) -> f64 {
        match self {
            ScoredItem::Track(_, s) => *s,
            ScoredItem::Album(_, s) => *s,
            ScoredItem::Artist(_, s) => *s,
        }
    }
}

/// get top results
/// 
/// returns the top results for the given query matching upstream behavior
#[get("/top")]
pub async fn get_top_results(query: web::Query<TopResultsQuery>) -> impl Responder {
    if query.q.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "No query provided"}));
    }

    let limit = query.limit;
    let tracks_limit = 4;

    // search all stores individually as each type has different scoring needs
    let track_results = SearchLib::search_tracks(&query.q, 150);
    let album_results = SearchLib::search_albums(&query.q, limit);
    let artist_results = SearchLib::search_artists(&query.q, limit);

    // combine all results and sort by score
    let mut all_results: Vec<ScoredItem> = Vec::new();
    for r in &artist_results {
        all_results.push(ScoredItem::Artist(r.item.clone(), r.score));
    }
    for r in &track_results {
        all_results.push(ScoredItem::Track(r.item.clone(), r.score));
    }
    for r in &album_results {
        all_results.push(ScoredItem::Album(r.item.clone(), r.score));
    }
    all_results.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));

    if all_results.is_empty() {
        return HttpResponse::Ok().json(TopResultsResponse {
            top_result: serde_json::json!(null),
            tracks: vec![],
            artists: vec![],
            albums: vec![],
        });
    }

    // get the top result
    let top_result = &all_results[0];
    let mut top_tracks: Vec<Track> = Vec::new();
    let mut top_albums: Vec<Album> = Vec::new();

    // get tracks and albums based on top result type (upstream behavior)
    match top_result {
        ScoredItem::Track(_track, _) => {
            // if top result is a track, top_tracks will be filled from search results
        }
        ScoredItem::Album(album, _) => {
            // if top result is an album, get tracks from that album
            let store = TrackStore::get();
            let album_tracks = store.get_by_album(&album.albumhash);
            let mut sorted_tracks: Vec<Track> = album_tracks.into_iter()
                .take(tracks_limit)
                .collect();
            sorted_tracks.sort_by(|a, b| b.playduration.cmp(&a.playduration));
            top_tracks = sorted_tracks;
        }
        ScoredItem::Artist(artist, _) => {
            // if top result is an artist, get tracks and albums from that artist
            let track_store = TrackStore::get();
            let artist_tracks = track_store.get_by_artist(&artist.artisthash);
            let mut sorted_tracks: Vec<Track> = artist_tracks.into_iter()
                .take(tracks_limit)
                .collect();
            sorted_tracks.sort_by(|a, b| b.playduration.cmp(&a.playduration));
            top_tracks = sorted_tracks;

            let album_store = AlbumStore::get();
            top_albums = album_store.get_by_artist(&artist.artisthash)
                .into_iter()
                .take(limit)
                .collect();
        }
    }

    // fill remaining tracks from search results if needed
    if top_tracks.len() < tracks_limit {
        let found_hashes: std::collections::HashSet<String> = top_tracks.iter()
            .map(|t| t.trackhash.clone())
            .collect();
        
        for result in &track_results {
            if !found_hashes.contains(&result.item.trackhash) {
                top_tracks.push(result.item.clone());
                if top_tracks.len() >= tracks_limit {
                    break;
                }
            }
        }
    }

    // fill remaining albums from search results if needed
    if top_albums.len() < limit {
        let found_hashes: std::collections::HashSet<String> = top_albums.iter()
            .map(|a| a.albumhash.clone())
            .collect();
        
        for result in &album_results {
            if !found_hashes.contains(&result.item.albumhash) {
                top_albums.push(result.item.clone());
                if top_albums.len() >= limit {
                    break;
                }
            }
        }
    }

    // serialize results
    let tracks_serialized: Vec<TrackSearchResult> = top_tracks.into_iter()
        .map(|t| t.into())
        .collect();

    let albums_serialized: Vec<AlbumSearchResult> = top_albums.into_iter()
        .map(|a| a.into())
        .collect();

    let artists_serialized: Vec<ArtistSearchResult> = artist_results.into_iter()
        .map(|r| r.item.into())
        .collect();

    // serialize top result with type field
    let top_result_json = match &all_results[0] {
        ScoredItem::Track(track, _) => {
            let mut result = serde_json::to_value(TrackSearchResult::from(track.clone())).unwrap();
            result.as_object_mut().unwrap().insert("type".to_string(), serde_json::json!("track"));
            result
        }
        ScoredItem::Album(album, _) => {
            let mut result = serde_json::to_value(AlbumSearchResult::from(album.clone())).unwrap();
            result.as_object_mut().unwrap().insert("type".to_string(), serde_json::json!("album"));
            result
        }
        ScoredItem::Artist(artist, _) => {
            let mut result = serde_json::to_value(ArtistSearchResult::from(artist.clone())).unwrap();
            result.as_object_mut().unwrap().insert("type".to_string(), serde_json::json!("artist"));
            result
        }
    };

    HttpResponse::Ok().json(TopResultsResponse {
        top_result: top_result_json,
        tracks: tracks_serialized,
        artists: artists_serialized,
        albums: albums_serialized,
    })
}

/// search items with pagination
///
/// find tracks, albums or artists from a search query with pagination support
#[get("")]
pub async fn search_items(query: web::Query<SearchLoadMoreQuery>) -> impl Responder {
    if query.q.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "No query provided"}));
    }

    match query.itemtype.as_str() {
        "tracks" => {
            let all_results = SearchLib::search_tracks(&query.q, 150);
            let total = all_results.len();
            let results: Vec<TrackSearchResult> = all_results.into_iter()
                .skip(query.start)
                .take(query.limit)
                .map(|r| r.item.into())
                .collect();
            let more = total > query.start + query.limit;
            
            HttpResponse::Ok().json(SearchLoadMoreResponse {
                results,
                more,
            })
        }
        "albums" => {
            let all_results = SearchLib::search_albums(&query.q, 150);
            let total = all_results.len();
            let results: Vec<AlbumSearchResult> = all_results.into_iter()
                .skip(query.start)
                .take(query.limit)
                .map(|r| r.item.into())
                .collect();
            let more = total > query.start + query.limit;
            
            HttpResponse::Ok().json(SearchLoadMoreResponse {
                results,
                more,
            })
        }
        "artists" => {
            let all_results = SearchLib::search_artists(&query.q, 150);
            let total = all_results.len();
            let results: Vec<ArtistSearchResult> = all_results.into_iter()
                .skip(query.start)
                .take(query.limit)
                .map(|r| r.item.into())
                .collect();
            let more = total > query.start + query.limit;
            
            HttpResponse::Ok().json(SearchLoadMoreResponse {
                results,
                more,
            })
        }
        _ => {
            HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid item type. Valid types are 'tracks', 'albums' and 'artists'"
            }))
        }
    }
}

/// configure search routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_top_results)
        .service(search_items);
}
