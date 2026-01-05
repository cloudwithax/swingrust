//! plugin management routes matching upstream behavior

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::config::UserConfig;
use crate::core::lyrics::LyricsLib;
use crate::db::tables::{PluginTable, UserTable};
use crate::models::{User, UserRole};
use crate::plugins::{LastFmPlugin, LyricsPlugin};
use crate::stores::TrackStore;
use crate::utils::auth::verify_jwt;
use crate::utils::hashing::create_hash;

/// list all plugins
#[get("")]
pub async fn get_plugins() -> impl Responder {
    match PluginTable::get_all().await {
        Ok(rows) => {
            let plugins: Vec<_> = rows
                .into_iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "active": p.active,
                        "settings": serde_json::from_str(&p.settings).unwrap_or(json!({})),
                        "extra": json!({}),
                    })
                })
                .collect();

            HttpResponse::Ok().json(json!({ "plugins": plugins }))
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to get plugins: {}", e) })),
    }
}

#[derive(Debug, Deserialize)]
pub struct PluginBody {
    pub plugin: String,
}

#[derive(Debug, Deserialize)]
pub struct PluginActivateBody {
    pub plugin: String,
    #[serde(default)]
    pub active: bool,
}

/// activate or deactivate a plugin (admin only)
#[post("/setactive")]
pub async fn activate_deactivate_plugin(
    req: HttpRequest,
    body: web::Json<PluginActivateBody>,
) -> impl Responder {
    if body.plugin.is_empty() {
        return HttpResponse::BadRequest().json(json!({"error": "Missing plugin"}));
    }

    if let Err(resp) = require_admin(&req).await {
        return resp;
    }

    if let Err(e) = PluginTable::set_active(&body.plugin, body.active).await {
        return HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to update plugin: {}", e) }));
    }

    HttpResponse::Ok().json(json!({"message": "OK"}))
}

#[derive(Debug, Deserialize)]
pub struct PluginSettingsBody {
    pub plugin: String,
    pub settings: serde_json::Value,
}

/// update plugin settings (admin only)
#[post("/settings")]
pub async fn update_plugin_settings(
    req: HttpRequest,
    body: web::Json<PluginSettingsBody>,
) -> impl Responder {
    if body.plugin.is_empty() || body.settings.is_null() {
        return HttpResponse::BadRequest().json(json!({"error": "Missing plugin or settings"}));
    }

    if let Err(resp) = require_admin(&req).await {
        return resp;
    }

    let settings_str = serde_json::to_string(&body.settings).unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = PluginTable::update_settings(&body.plugin, &settings_str).await {
        return HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to update settings: {}", e) }));
    }

    let plugin = PluginTable::get_by_name(&body.plugin).await.ok().flatten();

    let settings = plugin
        .and_then(|p| serde_json::from_str(&p.settings).ok())
        .unwrap_or(json!({}));

    HttpResponse::Ok().json(json!({"status": "success", "settings": settings }))
}

#[derive(Debug, Deserialize)]
pub struct LastFmSessionBody {
    pub token: String,
}

/// create a lastfm session and persist session key
#[post("/lastfm/session/create")]
pub async fn create_lastfm_session(
    req: HttpRequest,
    body: web::Json<LastFmSessionBody>,
) -> impl Responder {
    if body.token.is_empty() {
        return HttpResponse::BadRequest().json(json!({"error": "Missing token"}));
    }

    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let lastfm = LastFmPlugin::new();
    let session_key = lastfm.get_session_key(&body.token).await.ok();

    if let Some(key) = session_key.clone() {
        if let Ok(mut config) = UserConfig::load() {
            config.set_lastfm_session_key(user_id.to_string(), key);
            let _ = config.save();
        }
    }

    HttpResponse::Ok().json(json!({"status": "success", "session_key": session_key}))
}

/// delete the stored lastfm session for the user
#[post("/lastfm/session/delete")]
pub async fn delete_lastfm_session(req: HttpRequest) -> impl Responder {
    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    if let Ok(mut config) = UserConfig::load() {
        config.set_lastfm_session_key(user_id.to_string(), "".to_string());
        let _ = config.save();
    }

    HttpResponse::Ok().json(json!({"status": "success"}))
}

#[derive(Debug, Deserialize)]
pub struct LyricsSearchBody {
    pub trackhash: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub filepath: String,
    #[serde(default)]
    pub spotify_id: Option<String>,
}

/// search lyrics using musixmatch plugin
#[post("/lyrics/search")]
pub async fn search_lyrics(body: web::Json<LyricsSearchBody>) -> impl Responder {
    let plugin = LyricsPlugin::new();
    let track = TrackStore::get().get_by_hash(&body.trackhash);

    if let Some(spotify_id) = resolve_spotify_track_id(&body, track.as_ref().map(|t| &t.extra)) {
        if plugin.spotify_configured() {
            match plugin.download_spotify(&spotify_id, &body.filepath).await {
                Ok(Some(content)) => {
                    let lyrics = LyricsLib::parse_lrc(&content);
                    if lyrics.is_synced {
                        return HttpResponse::Ok()
                            .json(build_synced_response(&body.trackhash, &lyrics));
                    }
                }
                Ok(None) => {}
                Err(err) => warn!(
                    "spotify lyrics failed track_id={} error={:?}",
                    spotify_id, err
                ),
            }
        }
    }

    let results = match plugin.search(&body.title, &body.artist).await {
        Ok(res) => res,
        Err(err) => {
            warn!("musixmatch search failed error={:?}", err);
            Vec::new()
        }
    };

    if results.is_empty() {
        return HttpResponse::Ok()
            .json(json!({"trackhash": body.trackhash, "lyrics": serde_json::Value::Null}));
    }

    let mut perfect_match = results[0].clone();
    let target_title = create_hash(&[&body.title], true);
    let target_album = create_hash(&[&body.album], true);

    for track in results {
        let title_hash = create_hash(&[&track.title], true);
        let album_hash = create_hash(&[&track.album], true);
        if title_hash == target_title && album_hash == target_album {
            perfect_match = track;
            break;
        }
    }

    let lrc = match plugin
        .download(&perfect_match.track_id, &body.filepath)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            warn!("musixmatch download failed error={:?}", err);
            None
        }
    };

    let response = if let Some(ref content) = lrc {
        let lyrics = LyricsLib::parse_lrc(content);
        if lyrics.is_synced {
            build_synced_response(&body.trackhash, &lyrics)
        } else {
            json!({"trackhash": body.trackhash, "lyrics": serde_json::Value::Null})
        }
    } else {
        json!({"trackhash": body.trackhash, "lyrics": lrc})
    };

    HttpResponse::Ok().json(response)
}

fn build_synced_response(
    trackhash: &str,
    lyrics: &crate::core::lyrics::Lyrics,
) -> serde_json::Value {
    let formatted: Vec<_> = lyrics
        .lines
        .iter()
        .map(|line| {
            json!({
                "time": line.time.unwrap_or(0.0) * 1000.0,
                "text": line.text,
            })
        })
        .collect();

    json!({"trackhash": trackhash, "lyrics": formatted})
}

fn resolve_spotify_track_id(
    body: &LyricsSearchBody,
    extra: Option<&serde_json::Value>,
) -> Option<String> {
    if let Some(id) = body
        .spotify_id
        .as_ref()
        .and_then(|s| parse_spotify_track_id(s))
    {
        return Some(id);
    }

    if let Some(extra) = extra {
        if let Some(obj) = extra.as_object() {
            for key in [
                "spotify_id",
                "spotifyId",
                "spotify_track_id",
                "spotifyTrackId",
                "spotify",
                "spotifyUrl",
                "spotify_url",
                "spotifyUri",
                "spotify_uri",
            ] {
                if let Some(value) = obj.get(key) {
                    if let Some(id) = value.as_str().and_then(parse_spotify_track_id) {
                        return Some(id);
                    }

                    if let Some(nested) = value.get("id") {
                        if let Some(id) = nested.as_str().and_then(parse_spotify_track_id) {
                            return Some(id);
                        }
                    }
                }
            }
        }
    }

    None
}

fn parse_spotify_track_id(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim();

    if trimmed.len() >= 10
        && trimmed.len() <= 36
        && trimmed.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Some(trimmed.to_string());
    }

    static URL_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"spotify\.com/(?:track|embed/track)/([A-Za-z0-9]{10,})").unwrap());
    static URI_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"spotify:track:([A-Za-z0-9]{10,})").unwrap());

    if let Some(caps) = URL_RE.captures(trimmed) {
        return Some(caps[1].to_string());
    }

    if let Some(caps) = URI_RE.captures(trimmed) {
        return Some(caps[1].to_string());
    }

    None
}

/// configure plugin routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_plugins)
        .service(activate_deactivate_plugin)
        .service(update_plugin_settings)
        .service(create_lastfm_session)
        .service(delete_lastfm_session)
        .service(search_lyrics);
}

async fn resolve_user_id(req: &HttpRequest) -> Result<i64, HttpResponse> {
    match optional_user(req).await? {
        Some(user) => Ok(user.id),
        None => Ok(0),
    }
}

async fn require_admin(req: &HttpRequest) -> Result<User, HttpResponse> {
    let user = match optional_user(req).await? {
        Some(u) => u,
        None => {
            return Err(HttpResponse::Unauthorized().json(json!({"msg": "Not authenticated"})));
        }
    };

    if user.roles.contains(&UserRole::Admin) {
        Ok(user)
    } else {
        Err(HttpResponse::Forbidden().json(json!({"msg": "Only admins can do that!"})))
    }
}

async fn optional_user(req: &HttpRequest) -> Result<Option<User>, HttpResponse> {
    let header = match req.headers().get("Authorization") {
        Some(h) => h,
        None => return Ok(None),
    };

    let header_str = header.to_str().unwrap_or("");
    if !header_str.starts_with("Bearer ") {
        return Err(HttpResponse::Unauthorized().json(json!({"error": "Invalid token format"})));
    }
    let token = &header_str[7..];

    let config = UserConfig::load()
        .map_err(|_| HttpResponse::InternalServerError().json(json!({"error": "Config error"})))?;

    let claims = verify_jwt(token, &config.server_id, Some("access"))
        .map_err(|_| HttpResponse::Unauthorized().json(json!({"msg": "Invalid token"})))?;

    match UserTable::get_by_id(claims.sub.id).await {
        Ok(Some(user)) => Ok(Some(user)),
        Ok(None) => Err(HttpResponse::Unauthorized().json(json!({"msg": "Invalid token"}))),
        Err(_) => Err(HttpResponse::InternalServerError().json(json!({"msg": "Database error"}))),
    }
}
