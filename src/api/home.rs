//! Home API routes - homepage sections

use crate::config::UserConfig;
use crate::core::recipes::{ArtistStats, Recipes, RecentlyPlayedItem};
use crate::db::tables::{MixTable, ScrobbleTable};
use crate::models::Mix;
use crate::stores::{AlbumStore, ArtistStore, TrackStore};
use crate::utils::auth::verify_jwt;
use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const DEFAULT_USER_ID: i64 = 1;

/// Homepage section response
#[derive(Debug, Serialize)]
pub struct HomeSectionResponse {
    pub id: String,
    pub title: String,
    pub section_type: String,
    pub items: serde_json::Value,
    pub order_index: i32,
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

/// Configure home routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_recently_added_items)
        .service(get_recently_played_items)
        .service(nothome_homepage);
}

/// Upstream-compatible routes under /nothome
pub fn configure_upstream(cfg: &mut web::ServiceConfig) {
    cfg.service(nothome_homepage)
        .service(get_recently_added_items)
        .service(get_recently_played_items);
}

/// GET / (under /nothome) — return homepage items matching upstream format
#[get("/")]
async fn nothome_homepage(req: HttpRequest, query: web::Query<LimitQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(9);
    let user_id = resolve_user_id(&req).await.unwrap_or(DEFAULT_USER_ID);
    let payload = build_upstream_homepage_items(limit, user_id).await;

    HttpResponse::Ok().json(payload)
}

/// GET /recents/added (under /nothome)
#[get("/recents/added")]
async fn get_recently_added_items(query: web::Query<LimitQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(9) as usize;
    let items = build_recently_added_items(limit);
    HttpResponse::Ok().json(json!({ "items": items }))
}

/// GET /recents/played (under /nothome)
#[get("/recents/played")]
async fn get_recently_played_items(req: HttpRequest, query: web::Query<LimitQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(9) as usize;
    let user_id = resolve_user_id(&req).await.unwrap_or(DEFAULT_USER_ID);
    let items = build_recently_played(limit, user_id).await;
    HttpResponse::Ok().json(json!({ "items": items }))
}

// resolve user id from jwt token
async fn resolve_user_id(req: &HttpRequest) -> Option<i64> {
    let header = req.headers().get("Authorization")?;
    let header_str = header.to_str().ok()?.trim();
    if header_str.is_empty() {
        return None;
    }

    let token = if let Some(rest) = header_str.strip_prefix("Bearer ") {
        rest
    } else {
        header_str
    };
    if token.is_empty() {
        return None;
    }

    let config = UserConfig::load().ok()?;
    let claims = verify_jwt(token, &config.server_id, Some("access")).ok()?;
    Some(claims.sub.id)
}

// build the upstream-compatible homepage payload with all sections
async fn build_upstream_homepage_items(limit: usize, user_id: i64) -> Vec<Value> {
    let mut sections: Vec<Value> = Vec::new();
    let track_store = TrackStore::get();
    let album_store = AlbumStore::get();
    let artist_store = ArtistStore::get();

    // 1. recently played section (tracks, albums, artists, mixes, folders, playlists, favorites)
    let recently_played = Recipes::recently_played_items(limit, user_id).await;
    if !recently_played.is_empty() {
        let items = recover_recently_played_items(&recently_played);
        if !items.is_empty() {
            sections.push(json!({
                "recently_played": {
                    "title": "Recently played",
                    "description": "",
                    "items": items,
                }
            }));
        }
    }

    // 2. artist mixes for you
    let artist_mixes = Recipes::generate_artist_mixes(limit, user_id).await;
    if !artist_mixes.is_empty() {
        let items: Vec<Value> = artist_mixes
            .into_iter()
            .map(|mix| {
                json!({
                    "type": "mix",
                    "item": serialize_mix_for_homepage(&mix),
                })
            })
            .collect();

        sections.push(json!({
            "artist_mixes": {
                "title": "Artist mixes for you",
                "description": "Based on artists you have been listening to",
                "items": items,
            }
        }));
    }

    // 3. custom mixes (track-based mixes from database)
    if let Ok(db_mixes) = MixTable::all(0).await {
        // filter to track-type mixes (those starting with 't')
        let track_mixes: Vec<&Mix> = db_mixes
            .iter()
            .filter(|m| m.mixid.starts_with('t'))
            .take(limit)
            .collect();

        if !track_mixes.is_empty() {
            let items: Vec<Value> = track_mixes
                .into_iter()
                .map(|mix| {
                    json!({
                        "type": "mix",
                        "item": serialize_mix_for_homepage(mix),
                    })
                })
                .collect();

            sections.push(json!({
                "custom_mixes": {
                    "title": "Mixes for you",
                    "description": "Because artist mixes alone aren't enough",
                    "items": items,
                }
            }));
        }
    }

    // 4. daily mixes (spotify-style personalized playlists)
    let daily_mixes = Recipes::generate_daily_mixes(6, user_id).await;
    if !daily_mixes.is_empty() {
        let items: Vec<Value> = daily_mixes
            .into_iter()
            .map(|mix| {
                json!({
                    "type": "mix",
                    "item": serialize_mix_for_homepage(&mix),
                })
            })
            .collect();

        sections.push(json!({
            "daily_mixes": {
                "title": "Your Daily Mixes",
                "description": "Made for you based on what you've been listening to",
                "items": items,
            }
        }));
    }

    // 5. top artists this week
    let weekly_artists = Recipes::top_artists_weekly(limit, user_id).await;
    if !weekly_artists.is_empty() {
        let items: Vec<Value> = weekly_artists
            .iter()
            .filter_map(|stats| {
                let artist = artist_store.get_by_hash(&stats.artisthash)?;
                Some(json!({
                    "type": "artist",
                    "item": serialize_artist_for_homepage(&artist, stats),
                }))
            })
            .collect();

        if !items.is_empty() {
            sections.push(json!({
                "top_streamed_weekly_artists": {
                    "title": "Top artists this week",
                    "description": "Your most played artists since Monday",
                    "items": items,
                }
            }));
        }
    }

    // 6. top artists this month
    let monthly_artists = Recipes::top_artists_monthly(limit, user_id).await;
    if !monthly_artists.is_empty() {
        let items: Vec<Value> = monthly_artists
            .iter()
            .filter_map(|stats| {
                let artist = artist_store.get_by_hash(&stats.artisthash)?;
                Some(json!({
                    "type": "artist",
                    "item": serialize_artist_for_homepage(&artist, stats),
                }))
            })
            .collect();

        if !items.is_empty() {
            sections.push(json!({
                "top_streamed_monthly_artists": {
                    "title": "Top artists this month",
                    "description": "Your most played artists since the start of the month",
                    "items": items,
                }
            }));
        }
    }

    // 7. because you listened to (based on top artist)
    if let Some(first_artist) = Recipes::top_artists_in_period(7, 1, user_id).await.first() {
        if let Some(similar_section) = build_because_you_listened_section(&first_artist.artisthash, limit).await {
            sections.push(similar_section);
        }
    }

    // 8. artists you might like
    if let Some(artists_section) = build_artists_you_might_like(limit, user_id).await {
        sections.push(artists_section);
    }

    // 9. recently added albums (always last)
    let mut albums = album_store.get_all();
    albums.sort_by(|a, b| b.created_date.cmp(&a.created_date));
    let recently_added_albums: Vec<Value> = albums
        .into_iter()
        .take(limit)
        .filter_map(|a| {
            let album_value = serde_json::to_value(&a).ok()?;
            Some(json!({ "type": "album", "item": album_value }))
        })
        .collect();

    if !recently_added_albums.is_empty() {
        sections.push(json!({
            "recently_added": {
                "title": "Recently added",
                "description": "New music added to your library",
                "items": recently_added_albums,
            }
        }));
    }

    sections
}

// recover recently played items to full objects
fn recover_recently_played_items(items: &[RecentlyPlayedItem]) -> Vec<Value> {
    let track_store = TrackStore::get();
    let album_store = AlbumStore::get();
    let artist_store = ArtistStore::get();

    let mut recovered = Vec::new();

    for item in items {
        let recovered_item = match item.item_type.as_str() {
            "track" => {
                if let Some(track) = track_store.get_by_hash(&item.hash) {
                    let mut track_json = serde_json::to_value(&track).unwrap_or_default();
                    add_help_text(&mut track_json, &item.item_type, item.timestamp);
                    Some(json!({
                        "type": "track",
                        "item": track_json,
                    }))
                } else {
                    None
                }
            }
            "album" => {
                if let Some(album) = album_store.get_by_hash(&item.hash) {
                    let mut album_json = serde_json::to_value(&album).unwrap_or_default();
                    add_help_text(&mut album_json, &item.item_type, item.timestamp);
                    Some(json!({
                        "type": "album",
                        "item": album_json,
                    }))
                } else {
                    None
                }
            }
            "artist" => {
                if let Some(artist) = artist_store.get_by_hash(&item.hash) {
                    let mut artist_json = serde_json::to_value(&artist).unwrap_or_default();
                    add_help_text(&mut artist_json, &item.item_type, item.timestamp);
                    Some(json!({
                        "type": "artist",
                        "item": artist_json,
                    }))
                } else {
                    None
                }
            }
            "folder" => {
                let folder_count = count_tracks_in_folder(&item.hash);
                Some(json!({
                    "type": "folder",
                    "item": {
                        "path": item.hash,
                        "count": folder_count,
                        "help_text": "folder",
                        "time": timestamp_to_time_passed(item.timestamp),
                    },
                }))
            }
            "playlist" => {
                // for custom playlists like recentlyadded/recentlyplayed
                let is_custom = item.hash == "recentlyadded" || item.hash == "recentlyplayed";
                Some(json!({
                    "type": "playlist",
                    "item": {
                        "id": item.hash,
                        "is_custom": is_custom,
                        "help_text": "playlist",
                        "time": timestamp_to_time_passed(item.timestamp),
                    },
                }))
            }
            "mix" => {
                Some(json!({
                    "type": "mix",
                    "item": {
                        "id": item.hash,
                        "help_text": "mix",
                        "time": timestamp_to_time_passed(item.timestamp),
                    },
                }))
            }
            "favorite" => {
                let count = count_favorites();
                let image = get_favorite_image();
                Some(json!({
                    "type": "favorite",
                    "item": {
                        "count": count,
                        "image": image,
                        "help_text": "favorite",
                        "time": timestamp_to_time_passed(item.timestamp),
                    },
                }))
            }
            _ => None,
        };

        if let Some(item) = recovered_item {
            recovered.push(item);
        }
    }

    recovered
}

fn add_help_text(json: &mut Value, help_text: &str, timestamp: i64) {
    if let Some(obj) = json.as_object_mut() {
        obj.insert("help_text".to_string(), json!(help_text));
        obj.insert("time".to_string(), json!(timestamp_to_time_passed(timestamp)));
    }
}

fn timestamp_to_time_passed(timestamp: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let diff = now - timestamp;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if diff < 86400 {
        let hours = diff / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if diff < 604800 {
        let days = diff / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else if diff < 2629746 {
        let weeks = diff / 604800;
        if weeks == 1 {
            "1 week ago".to_string()
        } else {
            format!("{} weeks ago", weeks)
        }
    } else {
        let months = diff / 2629746;
        if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", months)
        }
    }
}

fn serialize_mix_for_homepage(mix: &Mix) -> Value {
    // get first track image if available
    let image = if !mix.images.is_empty() {
        Some(mix.images[0].clone())
    } else if !mix.trackhashes.is_empty() {
        TrackStore::get()
            .get_by_hash(&mix.trackhashes[0])
            .map(|t| t.image.clone())
    } else {
        None
    };

    json!({
        "id": mix.mixid,
        "title": mix.title,
        "description": mix.description,
        "trackcount": mix.trackhashes.len(),
        "image": image,
        "saved": mix.saved,
    })
}

fn serialize_artist_for_homepage(artist: &crate::models::Artist, stats: &ArtistStats) -> Value {
    json!({
        "artisthash": artist.artisthash,
        "name": artist.name,
        "image": artist.image,
        "trackcount": artist.trackcount,
        "albumcount": artist.albumcount,
        "play_count": stats.play_count,
        "help_text": format!("{} plays", stats.play_count),
    })
}

fn count_tracks_in_folder(path: &str) -> usize {
    TrackStore::get()
        .get_all()
        .iter()
        .filter(|t| t.filepath.starts_with(path))
        .count()
}

fn count_favorites() -> i64 {
    // count favorite tracks - would need FavoritesTable but return 0 for now
    0
}

fn get_favorite_image() -> Option<String> {
    None
}

async fn build_because_you_listened_section(artisthash: &str, limit: usize) -> Option<Value> {
    let artist = ArtistStore::get().get_by_hash(artisthash)?;

    // get tracks by this artist to find genres
    let artist_tracks = TrackStore::get().get_by_artist(artisthash);
    if artist_tracks.is_empty() {
        return None;
    }

    // collect genres
    let mut genre_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
    for track in &artist_tracks {
        for hash in &track.genrehashes {
            genre_hashes.insert(hash.clone());
        }
    }

    // find albums with similar genres
    let all_albums = AlbumStore::get().get_all();
    let mut similar_albums: Vec<_> = all_albums
        .into_iter()
        .filter(|a| {
            // album doesn't contain the main artist
            !a.artisthashes.contains(&artisthash.to_string())
        })
        .take(limit * 2)
        .collect();

    if similar_albums.is_empty() {
        return None;
    }

    similar_albums.truncate(limit);

    let items: Vec<Value> = similar_albums
        .iter()
        .map(|album| {
            json!({
                "type": "album",
                "item": serde_json::to_value(album).unwrap_or_default(),
            })
        })
        .collect();

    Some(json!({
        "because_you_listened_to_artist": {
            "title": format!("Because you listened to {}", artist.name),
            "description": "Artists similar to the artist you listened to",
            "items": items,
        }
    }))
}

async fn build_artists_you_might_like(limit: usize, user_id: i64) -> Option<Value> {
    // get top artists to find genres they belong to
    let top_artists = Recipes::top_artists_in_period(30, 5, user_id).await;
    if top_artists.is_empty() {
        return None;
    }

    // collect all genres from top artists
    let mut genre_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut top_artist_hashes: std::collections::HashSet<String> = std::collections::HashSet::new();

    for stats in &top_artists {
        top_artist_hashes.insert(stats.artisthash.clone());
        let tracks = TrackStore::get().get_by_artist(&stats.artisthash);
        for track in tracks {
            for hash in &track.genrehashes {
                genre_hashes.insert(hash.clone());
            }
        }
    }

    // find artists with similar genres that arent in top artists
    let all_artists = ArtistStore::get().get_all();
    let mut similar_artists: Vec<_> = all_artists
        .into_iter()
        .filter(|a| !top_artist_hashes.contains(&a.artisthash))
        .take(limit * 2)
        .collect();

    if similar_artists.is_empty() {
        return None;
    }

    similar_artists.truncate(limit);

    let items: Vec<Value> = similar_artists
        .iter()
        .map(|artist| {
            let track_label = if artist.trackcount == 1 {
                "1 track".to_string()
            } else {
                format!("{} tracks", artist.trackcount)
            };

            json!({
                "type": "artist",
                "item": {
                    "artisthash": artist.artisthash,
                    "name": artist.name,
                    "image": artist.image,
                    "trackcount": artist.trackcount,
                    "albumcount": artist.albumcount,
                    "help_text": track_label,
                },
            })
        })
        .collect();

    Some(json!({
        "artists_you_might_like": {
            "title": "Artists you might like",
            "description": "Artists similar to the artists you have listened to",
            "items": items,
        }
    }))
}

fn build_recently_added_items(limit: usize) -> Vec<Value> {
    let mut tracks = TrackStore::get().get_all();
    tracks.sort_by(|a, b| b.last_mod.cmp(&a.last_mod));
    tracks
        .into_iter()
        .take(limit)
        .map(|t| {
            json!({
                "type": "track",
                "hash": t.trackhash,
                "timestamp": t.last_mod,
                "help_text": "NEW TRACK"
            })
        })
        .collect()
}

async fn build_recently_played(limit: usize, user_id: i64) -> Vec<Value> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Ok(entries) = ScrobbleTable::get_paginated(user_id, 0, (limit as i64 * 5).max(50)).await {
        for entry in entries {
            if items.len() >= limit {
                break;
            }

            if seen.contains(&entry.trackhash) {
                continue;
            }
            seen.insert(entry.trackhash.clone());

            items.push(json!({
                "type": "track",
                "hash": entry.trackhash,
                "timestamp": entry.timestamp,
            }));
        }
    }

    items
}
