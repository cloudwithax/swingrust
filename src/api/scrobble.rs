//! Scrobble API routes

use actix_web::{get, post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::db::tables::ScrobbleTable;
use crate::stores::TrackStore;

/// Scrobble request
#[derive(Debug, Deserialize)]
pub struct ScrobbleRequest {
    pub trackhash: String,
    pub timestamp: Option<i64>,
    pub duration: Option<i32>,
}

/// Scrobble response
#[derive(Debug, Serialize)]
pub struct ScrobbleResponse {
    pub id: i64,
    pub trackhash: String,
    pub timestamp: i64,
    pub duration: i32,
}

/// Track scrobble stats
#[derive(Debug, Serialize)]
pub struct TrackScrobbleStats {
    pub trackhash: String,
    pub play_count: i64,
    pub last_played: Option<i64>,
}

/// Record scrobble
#[post("")]
pub async fn scrobble(body: web::Json<ScrobbleRequest>) -> impl Responder {
    let timestamp = body
        .timestamp
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    // Get track duration if not provided
    let duration = body.duration.unwrap_or_else(|| {
        TrackStore::get()
            .get_by_hash(&body.trackhash)
            .map(|t| t.duration)
            .unwrap_or(0)
    });

    match ScrobbleTable::insert(&body.trackhash, timestamp, duration).await {
        Ok(id) => HttpResponse::Created().json(ScrobbleResponse {
            id,
            trackhash: body.trackhash.clone(),
            timestamp,
            duration,
        }),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to record scrobble: {}", e)
        })),
    }
}

/// Get recent scrobbles
#[get("")]
pub async fn get_scrobbles(query: web::Query<PaginationQuery>) -> impl Responder {
    let page = query.page.unwrap_or(0) as i64;
    let limit = query.limit.unwrap_or(50) as i64;
    let start = page * limit;

    match ScrobbleTable::get_paginated_default(start, limit).await {
        Ok(scrobbles) => {
            let response: Vec<_> = scrobbles
                .into_iter()
                .map(|s| ScrobbleResponse {
                    id: s.id,
                    trackhash: s.trackhash,
                    timestamp: s.timestamp,
                    duration: s.duration,
                })
                .collect();

            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to get scrobbles: {}", e)
        })),
    }
}

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

/// Get scrobbles in time range
#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    pub start: i64,
    pub end: i64,
}

#[get("/range")]
pub async fn get_scrobbles_range(query: web::Query<TimeRangeQuery>) -> impl Responder {
    match ScrobbleTable::get_by_time_range(query.start, query.end).await {
        Ok(scrobbles) => {
            let response: Vec<_> = scrobbles
                .into_iter()
                .map(|s| ScrobbleResponse {
                    id: s.id,
                    trackhash: s.trackhash,
                    timestamp: s.timestamp,
                    duration: s.duration,
                })
                .collect();

            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to get scrobbles: {}", e)
        })),
    }
}

/// Get most recent scrobble
#[get("/recent")]
pub async fn get_most_recent() -> impl Responder {
    match ScrobbleTable::get_most_recent().await {
        Ok(Some(log)) => HttpResponse::Ok().json(ScrobbleResponse {
            id: log.id,
            trackhash: log.trackhash,
            timestamp: log.timestamp,
            duration: log.duration,
        }),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({
            "error": "No scrobbles found"
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to get scrobble: {}", e)
        })),
    }
}

/// Get play counts for tracks
#[get("/stats")]
pub async fn get_stats() -> impl Responder {
    match ScrobbleTable::get_all().await {
        Ok(scrobbles) => {
            // Count plays per track
            let mut play_counts: std::collections::HashMap<String, (i64, i64)> =
                std::collections::HashMap::new();

            for log in scrobbles {
                let entry = play_counts.entry(log.trackhash.clone()).or_insert((0, 0));
                entry.0 += 1;
                if log.timestamp > entry.1 {
                    entry.1 = log.timestamp;
                }
            }

            let stats: Vec<_> = play_counts
                .into_iter()
                .map(|(hash, (count, last))| TrackScrobbleStats {
                    trackhash: hash,
                    play_count: count,
                    last_played: if last > 0 { Some(last) } else { None },
                })
                .collect();

            HttpResponse::Ok().json(stats)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to get stats: {}", e)
        })),
    }
}

/// Configure scrobble routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(scrobble)
        .service(get_scrobbles)
        .service(get_scrobbles_range)
        .service(get_most_recent)
        .service(get_stats);
}
