//! Track-specific API routes

use actix_web::{delete, get, post, put, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::core::{tagger::Tagger, trackslib::TracksLib};
use crate::stores::TrackStore;

/// Single track hash path
#[derive(Debug, Deserialize)]
pub struct TrackPath {
    pub trackhash: String,
}

/// Multiple tracks request
#[derive(Debug, Deserialize)]
pub struct TracksRequest {
    pub trackhashes: Vec<String>,
}

/// Track metadata update request
#[derive(Debug, Deserialize, Serialize)]
pub struct TrackMetadataUpdate {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
}

/// Get track by hash
#[get("/{trackhash}")]
pub async fn get_track(path: web::Path<String>) -> impl Responder {
    let trackhash = path.into_inner();

    match TrackStore::get().get_by_hash(&trackhash) {
        Some(track) => HttpResponse::Ok().json(track),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Track not found"
        })),
    }
}

/// Get multiple tracks by hashes
#[post("/batch")]
pub async fn get_tracks_batch(body: web::Json<TracksRequest>) -> impl Responder {
    let store = TrackStore::get();
    let tracks: Vec<_> = body
        .trackhashes
        .iter()
        .filter_map(|h| store.get_by_hash(h))
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": tracks,
        "found": tracks.len(),
        "requested": body.trackhashes.len()
    }))
}

/// Get track file info
#[get("/{trackhash}/file")]
pub async fn get_track_file_info(path: web::Path<String>) -> impl Responder {
    let trackhash = path.into_inner();

    let track = match TrackStore::get().get_by_hash(&trackhash) {
        Some(t) => t,
        None => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Track not found"
            }));
        }
    };

    let file_path = std::path::Path::new(&track.filepath);

    let file_info = if file_path.exists() {
        let metadata = std::fs::metadata(file_path).ok();
        serde_json::json!({
            "exists": true,
            "size": metadata.as_ref().map(|m| m.len()),
            "path": track.filepath,
            "extension": file_path.extension().and_then(|e| e.to_str()),
            "filename": file_path.file_name().and_then(|f| f.to_str())
        })
    } else {
        serde_json::json!({
            "exists": false,
            "path": track.filepath
        })
    };

    HttpResponse::Ok().json(file_info)
}

/// Update track metadata (writes to file)
#[put("/{trackhash}/metadata")]
pub async fn update_track_metadata(
    path: web::Path<String>,
    body: web::Json<TrackMetadataUpdate>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let trackhash = path.into_inner();

    let track = match TrackStore::get().get_by_hash(&trackhash) {
        Some(t) => t,
        None => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Track not found"
            }));
        }
    };

    let file_path = std::path::Path::new(&track.filepath);

    if !file_path.exists() {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "Track file not found"
        }));
    }

    // Write metadata to file
    let tagger = Tagger;

    if let Some(ref title) = body.title {
        if let Err(e) = tagger.set_title(file_path, title) {
            tracing::error!("Failed to set title: {}", e);
        }
    }

    if let Some(ref artist) = body.artist {
        if let Err(e) = tagger.set_artist(file_path, artist) {
            tracing::error!("Failed to set artist: {}", e);
        }
    }

    if let Some(ref album) = body.album {
        if let Err(e) = tagger.set_album(file_path, album) {
            tracing::error!("Failed to set album: {}", e);
        }
    }

    if let Some(ref genre) = body.genre {
        if let Err(e) = tagger.set_genre(file_path, genre) {
            tracing::error!("Failed to set genre: {}", e);
        }
    }

    if let Some(year) = body.year {
        if let Err(e) = tagger.set_year(file_path, year as u32) {
            tracing::error!("Failed to set year: {}", e);
        }
    }

    // TODO: Update database and store
    // This would trigger a re-index of the track

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Metadata updated",
        "trackhash": trackhash
    }))
}

/// Delete track from library (removes from index, not file)
#[delete("/{trackhash}")]
pub async fn delete_track(path: web::Path<String>, pool: web::Data<SqlitePool>) -> impl Responder {
    let trackhash = path.into_inner();

    // Remove from store
    let removed = TrackStore::get().remove(&trackhash);

    if removed {
        // TODO: Remove from database
        // Also update album/artist stores

        HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "Track removed from library"
        }))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({
            "error": "Track not found"
        }))
    }
}

/// Get tracks by folder path
#[get("/folder")]
pub async fn get_tracks_by_folder(query: web::Query<FolderQuery>) -> impl Responder {
    let tracks = TracksLib::get_by_folder(&query.path);

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": tracks,
        "count": tracks.len(),
        "folder": &query.path
    }))
}

#[derive(Debug, Deserialize)]
pub struct FolderQuery {
    pub path: String,
}

/// Get recently added tracks
#[get("/recent")]
pub async fn get_recent_tracks(query: web::Query<RecentQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(50);
    let tracks = TracksLib::get_recent(limit);

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": tracks,
        "count": tracks.len()
    }))
}

#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    pub limit: Option<usize>,
}

/// Get random tracks
#[get("/random")]
pub async fn get_random_tracks(query: web::Query<RandomQuery>) -> impl Responder {
    use rand::seq::SliceRandom;

    let count = query.count.unwrap_or(20);
    let all_tracks = TrackStore::get().get_all();

    let mut rng = rand::thread_rng();
    let tracks: Vec<_> = all_tracks
        .choose_multiple(&mut rng, count.min(all_tracks.len()))
        .cloned()
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": tracks,
        "count": tracks.len()
    }))
}

#[derive(Debug, Deserialize)]
pub struct RandomQuery {
    pub count: Option<usize>,
}

/// Get track lyrics
#[get("/{trackhash}/lyrics")]
pub async fn get_track_lyrics(path: web::Path<String>) -> impl Responder {
    let trackhash = path.into_inner();

    let track = match TrackStore::get().get_by_hash(&trackhash) {
        Some(t) => t,
        None => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Track not found"
            }));
        }
    };

    // Try embedded lyrics first
    let file_path = std::path::Path::new(&track.filepath);
    if let Some(embedded) = crate::core::lyrics::LyricsLib::from_embedded(file_path) {
        let lyrics_text = crate::core::lyrics::LyricsLib::to_lrc(&embedded);
        if !lyrics_text.is_empty() {
            return HttpResponse::Ok().json(serde_json::json!({
                "source": "embedded",
                "lyrics": lyrics_text,
                "synced": embedded.is_synced
            }));
        }
    }

    // Fetch from external source
    use crate::core::lyrics::LyricsLib;

    match LyricsLib::fetch(
        &track.title,
        &track.artist(),
        Some(&track.album),
        track.duration as u64,
    )
    .await
    {
        Ok(lyrics) => HttpResponse::Ok().json(serde_json::json!({
            "source": "external",
            "lyrics": lyrics.lyrics,
            "synced": lyrics.synced
        })),
        Err(_) => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Lyrics not found"
        })),
    }
}

/// Configure track routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_track)
        .service(get_tracks_batch)
        .service(get_track_file_info)
        .service(update_track_metadata)
        .service(delete_track)
        .service(get_tracks_by_folder)
        .service(get_recent_tracks)
        .service(get_random_tracks)
        .service(get_track_lyrics);
}
