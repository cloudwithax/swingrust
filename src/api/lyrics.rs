//! lyrics api routes aligned with upstream flask behavior

use actix_web::{post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::core::lyrics::LyricsLib;
use crate::stores::TrackStore;

#[derive(Debug, Deserialize)]
pub struct SendLyricsBody {
    pub trackhash: String,
    pub filepath: String,
}

#[derive(Debug, Serialize)]
struct LyricsResponse {
    lyrics: serde_json::Value,
    synced: bool,
    copyright: String,
}

fn resolve_lyrics(body: &SendLyricsBody) -> Option<LyricsResponse> {
    let trackhash = &body.trackhash;
    let filepath = &body.filepath;

    let mut copyright = String::new();
    if let Some(track) = TrackStore::get().get_by_hash(trackhash) {
        if let Some(c) = track.copyright {
            copyright = c;
        }
    }

    // 1) .lrc / .rlrc
    if let Some(lyrics) = get_lyrics_file(filepath) {
        return Some(build_payload(lyrics, copyright));
    }

    // 2) tags
    if let Some(lyrics) = get_lyrics_from_tags(trackhash) {
        return Some(build_payload(lyrics, copyright));
    }

    // 3) duplicates (not implemented in rust store; kept for parity structure)
    if let Some(lyrics) = get_lyrics_from_duplicates(trackhash, filepath) {
        return Some(build_payload(lyrics, copyright));
    }

    None
}

/// returns lyrics for a track (file, tags, duplicates)
#[post("")]
pub async fn send_lyrics(body: web::Json<SendLyricsBody>) -> impl Responder {
    match resolve_lyrics(&body) {
        Some(payload) => HttpResponse::Ok().json(payload),
        None => HttpResponse::Ok().json(serde_json::json!({ "error": "No lyrics found" })),
    }
}

/// check if lyrics exist for a track
#[post("/check")]
pub async fn check_lyrics(body: web::Json<SendLyricsBody>) -> impl Responder {
    let exists = resolve_lyrics(&body).is_some();
    HttpResponse::Ok().json(serde_json::json!({ "exists": exists }))
}

fn get_lyrics_file(path: &str) -> Option<crate::core::lyrics::Lyrics> {
    let track_path = Path::new(path);
    let lrc_path = track_path.with_extension("lrc");
    let rlrc_path = track_path.with_extension("rlrc");

    if lrc_path.exists() {
        if let Ok(content) = fs::read_to_string(&lrc_path) {
            return Some(LyricsLib::parse_lrc(&content));
        }
    }

    if rlrc_path.exists() {
        if let Ok(content) = fs::read_to_string(&rlrc_path) {
            return Some(LyricsLib::parse_lrc(&content));
        }
    }

    None
}

fn get_lyrics_from_tags(trackhash: &str) -> Option<crate::core::lyrics::Lyrics> {
    let track = TrackStore::get().get_by_hash(trackhash)?;
    if let Some(lyrics_val) = track.extra.get("lyrics") {
        if let Some(text) = lyrics_val.as_str() {
            if LyricsLib::is_lrc_format(text) {
                return Some(LyricsLib::parse_lrc(text));
            }
            return Some(LyricsLib::parse_plain(text));
        }
    }
    None
}

fn get_lyrics_from_duplicates(
    _trackhash: &str,
    _filepath: &str,
) -> Option<crate::core::lyrics::Lyrics> {
    // rust store does not track duplicate filepaths per hash; kept for parity structure
    None
}

fn build_payload(lyrics: crate::core::lyrics::Lyrics, copyright: String) -> LyricsResponse {
    if lyrics.is_synced {
        let lines: Vec<_> = lyrics
            .lines
            .iter()
            .map(|line| {
                serde_json::json!({
                    "time": line.time.unwrap_or(0.0) * 1000.0,
                    "text": line.text,
                })
            })
            .collect();
        LyricsResponse {
            lyrics: serde_json::Value::Array(lines),
            synced: true,
            copyright,
        }
    } else {
        let lines: Vec<_> = lyrics
            .lines
            .iter()
            .map(|l| serde_json::Value::String(l.text.clone()))
            .collect();
        LyricsResponse {
            lyrics: serde_json::Value::Array(lines),
            synced: false,
            copyright,
        }
    }
}

/// configure lyrics routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(send_lyrics).service(check_lyrics);
}
