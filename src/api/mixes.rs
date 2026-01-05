//! Mixes API routes

use actix_web::{get, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::core::recipes::Recipes;

/// Mix response
#[derive(Debug, Serialize)]
pub struct MixResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub track_count: usize,
    pub image: Option<String>,
}

/// Mix track response
#[derive(Debug, Serialize)]
pub struct MixTrackResponse {
    pub trackhash: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub albumhash: String,
    pub duration: i32,
    pub image: String,
}

/// Get available mixes
#[get("")]
pub async fn get_mixes() -> impl Responder {
    let mixes = Recipes::get_homepage_mixes().await;

    let response: Vec<MixResponse> = mixes
        .into_iter()
        .map(|m| MixResponse {
            id: m.id,
            name: m.name,
            description: m.description,
            track_count: m.tracks.len(),
            image: m.image,
        })
        .collect();

    HttpResponse::Ok().json(response)
}

/// Get recently played
#[get("/recently-played")]
pub async fn get_recently_played(query: web::Query<LimitQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(20);
    let tracks = Recipes::recently_played(limit).await;

    let response: Vec<MixTrackResponse> = tracks
        .into_iter()
        .map(|t| {
            let artist = t.artist();
            MixTrackResponse {
                trackhash: t.trackhash,
                title: t.title,
                artist,
                album: t.album,
                albumhash: t.albumhash,
                duration: t.duration,
                image: t.image,
            }
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": response,
        "count": response.len()
    }))
}

/// Get recently added
#[get("/recently-added")]
pub async fn get_recently_added(query: web::Query<LimitQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(20);
    let tracks = Recipes::recently_added(limit);

    let response: Vec<MixTrackResponse> = tracks
        .into_iter()
        .map(|t| {
            let artist = t.artist();
            MixTrackResponse {
                trackhash: t.trackhash,
                title: t.title,
                artist,
                album: t.album,
                albumhash: t.albumhash,
                duration: t.duration,
                image: t.image,
            }
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": response,
        "count": response.len()
    }))
}

/// Get top streamed
#[get("/top-streamed")]
pub async fn get_top_streamed(query: web::Query<TopStreamedQuery>) -> impl Responder {
    let days = query.days.unwrap_or(30);
    let limit = query.limit.unwrap_or(20);
    let tracks = Recipes::top_streamed(days, limit).await;

    let response: Vec<MixTrackResponse> = tracks
        .into_iter()
        .map(|t| {
            let artist = t.artist();
            MixTrackResponse {
                trackhash: t.trackhash,
                title: t.title,
                artist,
                album: t.album,
                albumhash: t.albumhash,
                duration: t.duration,
                image: t.image,
            }
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "tracks": response,
        "count": response.len()
    }))
}

/// Get artist mix
#[get("/artist/{artisthash}")]
pub async fn get_artist_mix(
    path: web::Path<String>,
    query: web::Query<LimitQuery>,
) -> impl Responder {
    let artisthash = path.into_inner();
    let limit = query.limit.unwrap_or(30);

    match Recipes::artist_mix(&artisthash, limit) {
        Some(mix) => {
            let tracks: Vec<MixTrackResponse> = mix
                .tracks
                .into_iter()
                .map(|t| {
                    let artist = t.artist();
                    MixTrackResponse {
                        trackhash: t.trackhash,
                        title: t.title,
                        artist,
                        album: t.album,
                        albumhash: t.albumhash,
                        duration: t.duration,
                        image: t.image,
                    }
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "id": mix.id,
                "name": mix.name,
                "description": mix.description,
                "tracks": tracks,
                "image": mix.image
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Artist not found or no tracks"
        })),
    }
}

/// Get "because you listened to" mix
#[get("/because/{artisthash}")]
pub async fn get_because_mix(
    path: web::Path<String>,
    query: web::Query<LimitQuery>,
) -> impl Responder {
    let artisthash = path.into_inner();
    let limit = query.limit.unwrap_or(30);

    match Recipes::because_you_listened_to(&artisthash, limit).await {
        Some(mix) => {
            let tracks: Vec<MixTrackResponse> = mix
                .tracks
                .into_iter()
                .map(|t| {
                    let artist = t.artist();
                    MixTrackResponse {
                        trackhash: t.trackhash,
                        title: t.title,
                        artist,
                        album: t.album,
                        albumhash: t.albumhash,
                        duration: t.duration,
                        image: t.image,
                    }
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "id": mix.id,
                "name": mix.name,
                "description": mix.description,
                "tracks": tracks,
                "image": mix.image
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Artist not found or no similar tracks"
        })),
    }
}

/// Get genre mix
#[get("/genre/{genre}")]
pub async fn get_genre_mix(
    path: web::Path<String>,
    query: web::Query<LimitQuery>,
) -> impl Responder {
    let genre = path.into_inner();
    let limit = query.limit.unwrap_or(30);

    match Recipes::genre_mix(&genre, limit) {
        Some(mix) => {
            let tracks: Vec<MixTrackResponse> = mix
                .tracks
                .into_iter()
                .map(|t| {
                    let artist = t.artist();
                    MixTrackResponse {
                        trackhash: t.trackhash,
                        title: t.title,
                        artist,
                        album: t.album,
                        albumhash: t.albumhash,
                        duration: t.duration,
                        image: t.image,
                    }
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "id": mix.id,
                "name": mix.name,
                "description": mix.description,
                "tracks": tracks,
                "image": mix.image
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "No tracks found for this genre"
        })),
    }
}

/// Get decade mix
#[get("/decade/{decade}")]
pub async fn get_decade_mix(path: web::Path<i32>, query: web::Query<LimitQuery>) -> impl Responder {
    let decade = path.into_inner();
    let limit = query.limit.unwrap_or(30);

    match Recipes::decade_mix(decade, limit) {
        Some(mix) => {
            let tracks: Vec<MixTrackResponse> = mix
                .tracks
                .into_iter()
                .map(|t| {
                    let artist = t.artist();
                    MixTrackResponse {
                        trackhash: t.trackhash,
                        title: t.title,
                        artist,
                        album: t.album,
                        albumhash: t.albumhash,
                        duration: t.duration,
                        image: t.image,
                    }
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "id": mix.id,
                "name": mix.name,
                "description": mix.description,
                "tracks": tracks,
                "image": mix.image
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "No tracks found for this decade"
        })),
    }
}

/// Get random mix
#[get("/random")]
pub async fn get_random_mix(query: web::Query<LimitQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(30);
    let mix = Recipes::random_mix(limit);

    let tracks: Vec<MixTrackResponse> = mix
        .tracks
        .into_iter()
        .map(|t| {
            let artist = t.artist();
            MixTrackResponse {
                trackhash: t.trackhash,
                title: t.title,
                artist,
                album: t.album,
                albumhash: t.albumhash,
                duration: t.duration,
                image: t.image,
            }
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "id": mix.id,
        "name": mix.name,
        "description": mix.description,
        "tracks": tracks,
        "image": mix.image
    }))
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TopStreamedQuery {
    pub days: Option<i64>,
    pub limit: Option<usize>,
}

/// Configure mixes routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_mixes)
        .service(get_recently_played)
        .service(get_recently_added)
        .service(get_top_streamed)
        .service(get_artist_mix)
        .service(get_because_mix)
        .service(get_genre_mix)
        .service(get_decade_mix)
        .service(get_random_mix);
}
