//! GetAll API routes - match upstream Flask `/getall/<itemtype>` behavior

use actix_web::{get, web, HttpResponse, Responder};
use chrono::{Datelike, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::stores::{AlbumStore, ArtistStore};
use crate::utils::dates::{seconds_to_human_readable, timestamp_to_relative};

/// Query parameters (aligned with Python defaults/types)
#[derive(Debug, Deserialize)]
pub struct GetAllQuery {
    #[serde(default)]
    pub start: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_sort")]
    pub sortby: String,
    #[serde(default = "default_reverse")]
    pub reverse: String,
}

fn default_limit() -> usize {
    6
}

fn default_sort() -> String {
    "created_date".to_string()
}

fn default_reverse() -> String {
    "1".to_string()
}

/// Path param
#[derive(Debug, Deserialize)]
pub struct GetAllPath {
    pub itemtype: String,
}

/// GET /getall/<itemtype>
#[get("/{itemtype}")]
pub async fn get_all_items(
    path: web::Path<GetAllPath>,
    query: web::Query<GetAllQuery>,
) -> impl Responder {
    let is_albums = path.itemtype == "albums";
    let is_artists = path.itemtype == "artists";

    if !is_albums && !is_artists {
        return HttpResponse::BadRequest().json(json!({
            "error": "Invalid itemtype. Valid types are 'albums' or 'artists'",
        }));
    }

    let start = query.start;
    let limit = query.limit;
    let reverse = query.reverse == "1";
    let sort = query.sortby.as_str();

    if is_albums {
        let mut items = AlbumStore::get().get_all();
        sort_albums(&mut items, sort, reverse);
        let total = items.len();
        let slice = items
            .into_iter()
            .skip(start)
            .take(limit)
            .collect::<Vec<_>>();
        let mapped = slice
            .into_iter()
            .map(|mut a| {
                let mut map = to_album_card_map(&mut a);
                if let Some(help) = album_help_text(sort, &a) {
                    map.insert("help_text".to_string(), Value::String(help));
                }
                Value::Object(map)
            })
            .collect::<Vec<_>>();

        return HttpResponse::Ok().json(json!({
            "items": mapped,
            "total": total,
        }));
    }

    let mut items = ArtistStore::get().get_all();
    sort_artists(&mut items, sort, reverse);
    let total = items.len();
    let slice = items
        .into_iter()
        .skip(start)
        .take(limit)
        .collect::<Vec<_>>();
    let mapped = slice
        .into_iter()
        .map(|mut a| {
            let mut map = to_artist_card_map(&mut a);
            if let Some(help) = artist_help_text(sort, &a) {
                map.insert("help_text".to_string(), Value::String(help));
            }
            Value::Object(map)
        })
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(json!({
        "items": mapped,
        "total": total,
    }))
}

fn sort_albums(items: &mut [crate::models::Album], sort: &str, reverse: bool) {
    items.sort_by(|a, b| {
        let ord = match sort {
            "duration" => a.duration.cmp(&b.duration),
            "created_date" => a.created_date.cmp(&b.created_date),
            "playcount" => a.playcount.cmp(&b.playcount),
            "playduration" => a.playduration.cmp(&b.playduration),
            "lastplayed" => a.lastplayed.cmp(&b.lastplayed),
            "trackcount" => a.trackcount.cmp(&b.trackcount),
            "date" => a.date.cmp(&b.date),
            "albumartists" => a
                .albumartists
                .get(0)
                .and_then(|ar| Some(ar.name.to_lowercase()))
                .cmp(
                    &b.albumartists
                        .get(0)
                        .and_then(|ar| Some(ar.name.to_lowercase())),
                ),
            "title" => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            _ => a.created_date.cmp(&b.created_date),
        };
        if reverse {
            ord.reverse()
        } else {
            ord
        }
    });
}

fn sort_artists(items: &mut [crate::models::Artist], sort: &str, reverse: bool) {
    items.sort_by(|a, b| {
        let ord = match sort {
            "duration" => a.duration.cmp(&b.duration),
            "created_date" => a.created_date.cmp(&b.created_date),
            "playcount" => a.playcount.cmp(&b.playcount),
            "playduration" => a.playduration.cmp(&b.playduration),
            "lastplayed" => a.lastplayed.cmp(&b.lastplayed),
            "trackcount" => a.trackcount.cmp(&b.trackcount),
            "albumcount" => a.albumcount.cmp(&b.albumcount),
            "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            _ => a.created_date.cmp(&b.created_date),
        };
        if reverse {
            ord.reverse()
        } else {
            ord
        }
    });
}

pub fn to_album_card_map(album: &mut crate::models::Album) -> Map<String, Value> {
    let mut value = serde_json::to_value(&*album)
        .unwrap_or_else(|_| json!({}))
        .as_object()
        .cloned()
        .unwrap_or_default();

    for key in [
        "duration",
        "count",
        "artisthashes",
        "albumartists_hashes",
        "created_date",
        "og_title",
        "base_title",
        "genres",
        "playcount",
        "trackcount",
        "type",
        "playduration",
        "genrehashes",
        "fav_userids",
        "extra",
        "id",
        "lastplayed",
        "weakhash",
        "help_text",
    ] {
        value.remove(key);
    }

    let dynamic_remove: Vec<String> = value
        .keys()
        .filter(|k| k.starts_with("is_"))
        .cloned()
        .collect();
    for key in dynamic_remove {
        value.remove(&key);
    }

    if let Some(arr) = value.get_mut("albumartists").and_then(|v| v.as_array_mut()) {
        for artist in arr {
            if let Some(obj) = artist.as_object_mut() {
                obj.remove("image");
            }
        }
    }

    // ensure image/pathhash fields
    if let (Some(image), Some(pathhash)) = (value.get("image"), value.get("pathhash")) {
        if image.is_null() || image.as_str().unwrap_or("").is_empty() {
            let path = pathhash.as_str().unwrap_or(&album.albumhash);
            let img = format!("{}.webp?pathhash={}", album.albumhash, path);
            value.insert("image".to_string(), Value::String(img));
        }
    } else {
        let path = if album.pathhash.is_empty() {
            album.albumhash.clone()
        } else {
            album.pathhash.clone()
        };
        let img = format!("{}.webp?pathhash={}", album.albumhash, path);
        value.insert("pathhash".to_string(), Value::String(path.clone()));
        value.insert("image".to_string(), Value::String(img));
    }

    value.insert("type".to_string(), Value::String("album".to_string()));

    value
}

pub fn to_artist_card_map(artist: &mut crate::models::Artist) -> Map<String, Value> {
    let mut map = serde_json::to_value(artist)
        .unwrap_or(json!({}))
        .as_object()
        .cloned()
        .unwrap_or_default();
    for key in [
        "is_favorite",
        "trackcount",
        "duration",
        "albumcount",
        "playcount",
        "playduration",
        "lastplayed",
        "id",
        "genres",
        "genrehashes",
        "extra",
        "created_date",
        "date",
        "fav_userids",
        "score",
        "type",
    ] {
        map.remove(key);
    }

    map.insert("type".to_string(), Value::String("artist".to_string()));
    map
}

fn album_help_text(sort: &str, album: &crate::models::Album) -> Option<String> {
    match sort {
        "date" => {
            if album.date > 0 {
                let year = Utc.timestamp_opt(album.date as i64, 0).single()?.year();
                Some(year.to_string())
            } else {
                None
            }
        }
        "created_date" => Some(timestamp_to_relative(album.created_date)),
        "trackcount" => Some(format!(
            "{} track{}",
            format_number(album.trackcount as i64),
            if album.trackcount == 1 { "" } else { "s" }
        )),
        "duration" => Some(seconds_to_human_readable(album.duration as i64)),
        "playcount" => Some(format!(
            "{} play{}",
            format_number(album.playcount as i64),
            if album.playcount == 1 { "" } else { "s" }
        )),
        "lastplayed" => {
            if album.playduration == 0 {
                Some("Never played".to_string())
            } else {
                Some(timestamp_to_relative(album.lastplayed))
            }
        }
        "playduration" => Some(seconds_to_human_readable(album.playduration as i64)),
        _ => None,
    }
}

fn artist_help_text(sort: &str, artist: &crate::models::Artist) -> Option<String> {
    match sort {
        "trackcount" => Some(format!(
            "{} track{}",
            format_number(artist.trackcount as i64),
            if artist.trackcount == 1 { "" } else { "s" }
        )),
        "albumcount" => Some(format!(
            "{} album{}",
            format_number(artist.albumcount as i64),
            if artist.albumcount == 1 { "" } else { "s" }
        )),
        "playcount" => Some(format!(
            "{} play{}",
            format_number(artist.playcount as i64),
            if artist.playcount == 1 { "" } else { "s" }
        )),
        "lastplayed" => {
            if artist.playduration == 0 {
                Some("Never played".to_string())
            } else {
                Some(timestamp_to_relative(artist.lastplayed))
            }
        }
        "playduration" => Some(seconds_to_human_readable(artist.playduration as i64)),
        _ => None,
    }
}

fn format_number(n: i64) -> String {
    let mut s = n.abs().to_string();
    let mut res = String::new();
    while s.len() > 3 {
        let split = s.split_off(s.len() - 3);
        res = format!(",{}{}", split, res);
    }
    res = format!("{}{}", s, res);
    if n < 0 {
        format!("-{}", res)
    } else {
        res
    }
}

/// Configure getall routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_all_items);
}
