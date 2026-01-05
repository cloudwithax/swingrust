//! mixes plugin routes matching python upstream behavior

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::config::UserConfig;
use crate::db::tables::{MixTable, UserTable};
use crate::models::{Mix, Track, User};
use crate::stores::TrackStore;
use crate::utils::auth::verify_jwt;
use crate::utils::dates::timestamp_to_relative;
use crate::utils::hashing::create_hash;

#[derive(Debug, Deserialize)]
pub struct MixTypePath {
    pub mixtype: String,
}

#[derive(Debug, Deserialize)]
pub struct MixQuery {
    pub mixid: String,
    pub sourcehash: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveMixRequest {
    pub mixid: String,
    #[serde(rename = "type")]
    pub mix_type: String,
    pub sourcehash: String,
}

/// GET /plugins/mixes/<mixtype>
#[get("/{mixtype}")]
pub async fn get_mixes(req: HttpRequest, path: web::Path<MixTypePath>) -> impl Responder {
    let user = match require_user(&req).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    let mixes = match MixTable::all(user.id).await {
        Ok(list) => list,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to fetch mixes: {}", e) }))
        }
    };

    let mut items: Vec<Value> = Vec::new();
    for mix in mixes {
        match path.mixtype.as_str() {
            "artists" => {
                items.push(serialize_mix_compact(&mix, true));
            }
            "tracks" => {
                // upstream wraps artist mixes into track mixes when available
                items.push(serialize_mix_compact(&mix, true));
            }
            _ => {
                return HttpResponse::BadRequest().json(json!({ "msg": "Invalid mix type" }));
            }
        }
    }

    // dedupe for track mixes only
    if path.mixtype == "tracks" {
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                if seen.contains(id) {
                    return false;
                }
                seen.insert(id.to_string());
            }
            true
        });
    }

    HttpResponse::Ok().json(items)
}

/// GET /plugins/mixes?mixid&sourcehash
#[get("")]
pub async fn get_mix(req: HttpRequest, query: web::Query<MixQuery>) -> impl Responder {
    let user = match require_user(&req).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    let mix_type = match query.mixid.chars().next() {
        Some('a') => "artist_mixes",
        Some('t') => "custom_mixes",
        _ => {
            return HttpResponse::BadRequest().json(json!({ "msg": "Invalid mix ID" }));
        }
    };

    let mix = match MixTable::get_by_sourcehash(&query.sourcehash, user.id).await {
        Ok(Some(m)) => m,
        Ok(None) => return HttpResponse::NotFound().json(json!({ "msg": "Mix not found" })),
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to fetch mix: {}", e) }))
        }
    };

    // upstream may transform custom mixes; we return stored mix as-is
    let full = serialize_mix_full(&mix, mix_type == "custom_mixes", user.id);
    HttpResponse::Ok().json(full)
}

/// POST /plugins/mixes/save
#[post("/save")]
pub async fn save_mix(req: HttpRequest, body: web::Json<SaveMixRequest>) -> impl Responder {
    let user = match require_user(&req).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    let state = match body.mix_type.as_str() {
        "artist" => MixTable::save_artist_mix(&body.sourcehash, user.id).await,
        "track" => MixTable::save_track_mix(&body.sourcehash, user.id).await,
        _ => {
            return HttpResponse::BadRequest().json(json!({ "msg": "Invalid mix type" }));
        }
    };

    match state {
        Ok(_) => HttpResponse::Ok().json(json!({ "msg": "Mixes saved" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to save mix: {}", e) })),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_mixes).service(get_mix).service(save_mix);
}

fn serialize_mix_compact(mix: &Mix, convert_time: bool) -> Value {
    let trackshash = create_hash(
        &mix.trackhashes
            .iter()
            .take(40)
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        true,
    );

    let mut map = Map::new();
    map.insert("id".to_string(), json!(mix.mixid));
    map.insert("title".to_string(), json!(mix.title));
    map.insert("description".to_string(), json!(mix.description));
    map.insert("timestamp".to_string(), json!(mix.timestamp));
    map.insert("trackshash".to_string(), json!(trackshash));
    map.insert("type".to_string(), json!("mix"));
    map.insert("saved".to_string(), json!(mix.saved));
    map.insert("userid".to_string(), json!(mix.userid));
    map.insert("sourcehash".to_string(), json!(mix.sourcehash));
    map.insert("extra".to_string(), clean_extra(mix.extra.clone()));

    if convert_time {
        map.insert(
            "time".to_string(),
            json!(timestamp_to_relative(mix.timestamp)),
        );
    }

    Value::Object(map)
}

fn serialize_mix_full(mix: &Mix, _is_custom: bool, user_id: i64) -> Value {
    let tracks = TrackStore::get().get_by_hashes(&mix.trackhashes);
    let serialized_tracks: Vec<Value> = tracks
        .iter()
        .map(|t| serialize_track_for_mix(t, user_id))
        .collect();
    let total_duration: i32 = tracks.iter().map(|t| t.duration).sum();

    let mut map = Map::new();
    map.insert("id".to_string(), json!(mix.mixid));
    map.insert("title".to_string(), json!(mix.title));
    map.insert("description".to_string(), json!(mix.description));
    map.insert("tracks".to_string(), Value::Array(serialized_tracks));
    map.insert("sourcehash".to_string(), json!(mix.sourcehash));
    map.insert("userid".to_string(), json!(mix.userid));
    map.insert("timestamp".to_string(), json!(mix.timestamp));
    map.insert("saved".to_string(), json!(mix.saved));
    map.insert("extra".to_string(), clean_extra(mix.extra.clone()));
    map.insert(
        "duration".to_string(),
        json!(seconds_to_time_string(total_duration as i64)),
    );
    map.insert("trackcount".to_string(), json!(mix.trackhashes.len()));

    Value::Object(map)
}

fn clean_extra(extra: Value) -> Value {
    match extra {
        Value::Object(mut obj) => {
            obj.remove("albums");
            obj.remove("artists");
            Value::Object(obj)
        }
        _ => json!({}),
    }
}

fn serialize_track_for_mix(track: &Track, user_id: i64) -> Value {
    let mut value = serde_json::to_value(track).unwrap_or_else(|_| json!({}));
    if let Some(map) = value.as_object_mut() {
        let mut to_remove: std::collections::HashSet<String> = [
            "date",
            "genre",
            "last_mod",
            "og_title",
            "og_album",
            "copyright",
            "config",
            "artist_hashes",
            "created_date",
            "fav_userids",
            "playcount",
            "genrehashes",
            "id",
            "lastplayed",
            "playduration",
            "genres",
            "score",
            "help_text",
            "pos",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        to_remove.insert("disc".to_string());
        to_remove.insert("track".to_string());

        let dynamic_remove: Vec<String> = map
            .keys()
            .filter(|k| k.starts_with('_') || k.starts_with("is_"))
            .cloned()
            .collect();
        for key in dynamic_remove {
            to_remove.insert(key);
        }

        for key in to_remove {
            map.remove(&key);
        }

        for key in ["artists", "albumartists"] {
            if let Some(Value::Array(items)) = map.get_mut(key) {
                for artist in items {
                    if let Some(obj) = artist.as_object_mut() {
                        obj.remove("image");
                    }
                }
            }
        }

        map.insert(
            "is_favorite".to_string(),
            Value::Bool(track.is_favorite(user_id)),
        );
    }

    value
}

fn seconds_to_time_string(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

async fn require_user(req: &HttpRequest) -> Result<User, HttpResponse> {
    let token = match access_token(req) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Err(HttpResponse::Unauthorized().json(json!({
                "msg": "Not authenticated"
            })));
        }
        Err(resp) => return Err(resp),
    };

    let config = match UserConfig::load() {
        Ok(cfg) => cfg,
        Err(_) => {
            return Err(HttpResponse::InternalServerError().json(json!({
                "error": "Config error"
            })));
        }
    };

    let claims = match verify_jwt(&token, &config.server_id, Some("access")) {
        Ok(c) => c,
        Err(_) => {
            return Err(HttpResponse::Unauthorized().json(json!({
                "msg": "Invalid token"
            })));
        }
    };

    match UserTable::get_by_id(claims.sub.id).await {
        Ok(Some(user)) => Ok(user),
        Ok(None) => Err(HttpResponse::Unauthorized().json(json!({
            "msg": "Invalid token"
        }))),
        Err(_) => Err(HttpResponse::InternalServerError().json(json!({
            "msg": "Database error"
        }))),
    }
}

fn access_token(req: &HttpRequest) -> Result<Option<String>, HttpResponse> {
    if let Some(cookie) = req.cookie("access_token_cookie") {
        return Ok(Some(cookie.value().to_string()));
    }

    match req.headers().get("Authorization") {
        Some(header_value) => {
            let header_str = header_value.to_str().unwrap_or("");
            if !header_str.starts_with("Bearer ") {
                return Err(
                    HttpResponse::Unauthorized().json(json!({ "error": "Invalid token format" }))
                );
            }
            Ok(Some(header_str[7..].to_string()))
        }
        None => Ok(None),
    }
}
