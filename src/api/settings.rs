//! Settings API routes

use actix_web::{get, post, put, web, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::config::UserConfig;
use crate::db::tables::{PluginTable, UserTable};
use crate::utils::auth::verify_jwt;

/// Settings response
#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub root_dirs: Vec<String>,
    pub artist_separators: Vec<String>,
}

impl From<&UserConfig> for SettingsResponse {
    fn from(config: &UserConfig) -> Self {
        let mut separators: Vec<String> = config.artist_separators.iter().cloned().collect();
        separators.sort();
        Self {
            root_dirs: config.root_dirs.clone(),
            artist_separators: separators,
        }
    }
}

/// Update settings request
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub root_dirs: Option<Vec<String>>,
    pub artist_separators: Option<Vec<String>>,
}

/// Get settings
#[get("")]
pub async fn get_settings() -> impl Responder {
    match UserConfig::load() {
        Ok(config) => HttpResponse::Ok().json(SettingsResponse::from(&config)),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to load settings: {}", e)
        })),
    }
}

/// Update settings
#[put("")]
pub async fn update_settings(body: web::Json<UpdateSettingsRequest>) -> impl Responder {
    let mut config = match UserConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to load settings: {}", e)
            }));
        }
    };

    // Update fields if provided
    if let Some(dirs) = &body.root_dirs {
        config.root_dirs = dirs.clone();
    }

    if let Some(separators) = &body.artist_separators {
        config.artist_separators = separators.iter().cloned().collect();
    }

    // Save settings
    match config.save() {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "message": "Settings updated"
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to save settings: {}", e)
        })),
    }
}

/// Add root directory
#[post("/root-dirs")]
pub async fn add_root_dir(body: web::Json<AddRootDirRequest>) -> impl Responder {
    let mut config = match UserConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to load settings: {}", e)
            }));
        }
    };

    // Validate path exists
    if !std::path::Path::new(&body.path).exists() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Directory does not exist"
        }));
    }

    // Add if not already present
    if !config.root_dirs.contains(&body.path) {
        config.root_dirs.push(body.path.clone());

        if let Err(e) = config.save() {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to save settings: {}", e)
            }));
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Root directory added",
        "root_dirs": config.root_dirs
    }))
}

/// Add root dir request
#[derive(Debug, Deserialize)]
pub struct AddRootDirRequest {
    pub path: String,
}

/// Remove root directory
#[derive(Debug, Deserialize)]
pub struct RemoveRootDirRequest {
    pub path: String,
}

#[post("/root-dirs/remove")]
pub async fn remove_root_dir(body: web::Json<RemoveRootDirRequest>) -> impl Responder {
    let mut config = match UserConfig::load() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to load settings: {}", e)
            }));
        }
    };

    config.root_dirs.retain(|d| d != &body.path);

    if let Err(e) = config.save() {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to save settings: {}", e)
        }));
    }

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Root directory removed",
        "root_dirs": config.root_dirs
    }))
}

/// Trigger library rescan
#[post("/rescan")]
pub async fn rescan_library() -> impl Responder {
    match UserConfig::load() {
        Ok(config) => {
            if config.root_dirs.is_empty() {
                warn!("Scan requested but no root directories are configured");
                return HttpResponse::Ok().json(serde_json::json!({
                    "message": "No root directories configured"
                }));
            }

            spawn_library_scan(config, false);

            HttpResponse::Ok().json(serde_json::json!({
                "message": "Library rescan initiated"
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to load settings: {}", e)
        })),
    }
}

/// Configure settings routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_settings)
        .service(update_settings)
        .service(add_root_dir)
        .service(remove_root_dir)
        .service(rescan_library);
}

// ---------- Upstream-compatible routes under /notsettings ----------

#[derive(Debug, Deserialize)]
pub struct AddRootDirsBody {
    pub new_dirs: Vec<String>,
    pub removed: Vec<String>,
}

#[post("/add-root-dirs")]
pub async fn add_root_dirs(body: web::Json<AddRootDirsBody>) -> impl Responder {
    let mut config = match UserConfig::load() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to load config"
            }));
        }
    };

    let home_token = "$home".to_string();
    let db_dirs = config.root_dirs.clone();
    let mut new_dirs = body.new_dirs.clone();
    let mut removed_dirs = body.removed.clone();

    let db_home = db_dirs.iter().any(|d| d == &home_token);
    let incoming_home = new_dirs.iter().any(|d| d == &home_token);

    // both have $home -> not modified
    if db_home && incoming_home {
        return HttpResponse::NotModified().finish();
    }

    // if $home present either side, clear others
    if db_home || incoming_home {
        config.root_dirs.clear();
    }

    if incoming_home {
        config.root_dirs = vec![home_token];
        let _ = config.save();
        spawn_library_scan(config, false);
        return HttpResponse::Ok().json(serde_json::json!({ "root_dirs": vec!["$home"] }));
    }

    // remove child dirs of incoming additions
    for dir in &new_dirs {
        let children: Vec<String> = db_dirs
            .iter()
            .filter(|d| d.starts_with(dir) && *d != dir)
            .cloned()
            .collect();
        removed_dirs.extend(children);
    }

    let mut updated_dirs = db_dirs
        .into_iter()
        .filter(|d| !removed_dirs.contains(d))
        .collect::<Vec<_>>();

    for dir in new_dirs.drain(..) {
        if !updated_dirs.contains(&dir) && dir != home_token {
            updated_dirs.push(dir);
        }
    }

    config.root_dirs = updated_dirs.clone();

    if let Err(_) = config.save() {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to save config"
        }));
    }

    spawn_library_scan(config, false);

    HttpResponse::Ok().json(serde_json::json!({
        "root_dirs": updated_dirs
    }))
}

#[get("/get-root-dirs")]
pub async fn get_root_dirs_upstream() -> impl Responder {
    match UserConfig::load() {
        Ok(config) => HttpResponse::Ok().json(serde_json::json!({
            "dirs": config.root_dirs
        })),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to load config"
        })),
    }
}

#[get("")]
pub async fn get_all_settings_upstream(req: HttpRequest) -> impl Responder {
    let config = match UserConfig::load() {
        Ok(cfg) => cfg,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to load config"
            }));
        }
    };

    // convert sets to lists to match python response
    let mut config_value = serde_json::to_value(&config).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = config_value.as_object_mut() {
        for key in [
            "artistSeparators",
            "artistSplitIgnoreList",
            "genreSeparators",
        ] {
            if let Some(val) = obj.get_mut(key) {
                if let Some(set) = val.as_array_mut() {
                    set.sort_by(|a, b| {
                        a.as_str()
                            .unwrap_or_default()
                            .cmp(b.as_str().unwrap_or_default())
                    });
                }
            }
        }
    }

    // add plugins
    let plugins = PluginTable::get_all().await.ok().map(|rows| {
        rows.into_iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "active": p.active,
                    "settings": serde_json::from_str::<serde_json::Value>(&p.settings).unwrap_or_else(|_| serde_json::json!({})),
                    "extra": serde_json::json!({})
                })
            })
            .collect::<Vec<_>>()
    });

    if let Some(obj) = config_value.as_object_mut() {
        obj.insert(
            "plugins".to_string(),
            serde_json::json!(plugins.unwrap_or_default()),
        );
        obj.insert(
            "version".to_string(),
            serde_json::json!(env!("CARGO_PKG_VERSION")),
        );
    }

    // expose only current user's lastfm session key
    if let Some(obj) = config_value.as_object_mut() {
        if let Some(user_id) = resolve_user_id(&req).await {
            let key = config
                .lastfm_session_keys
                .get(&user_id.to_string())
                .cloned()
                .unwrap_or_default();
            obj.insert("lastfmSessionKey".to_string(), serde_json::json!(key));
        } else {
            obj.insert("lastfmSessionKey".to_string(), serde_json::json!(""));
        }
        obj.remove("lastfmSessionKeys");
    }

    HttpResponse::Ok().json(config_value)
}

#[get("/trigger-scan")]
pub async fn trigger_scan_upstream() -> impl Responder {
    match UserConfig::load() {
        Ok(config) => {
            if config.root_dirs.is_empty() {
                warn!("Scan requested but no root directories are configured");
                return HttpResponse::Ok().json(serde_json::json!({
                    "msg": "No root directories configured"
                }));
            }

            spawn_library_scan(config, false);
        }
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": format!("Failed to load config: {}", e)
            }));
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "msg": "Scan triggered!"
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigBody {
    pub key: String,
    pub value: serde_json::Value,
}

#[put("/update")]
pub async fn update_config_upstream(body: web::Json<UpdateConfigBody>) -> impl Responder {
    let mut config = match UserConfig::load() {
        Ok(c) => c,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to load config"
            }));
        }
    };

    // Attempt to set field dynamically
    let key = body.key.as_str();
    let val = body.value.clone();
    let mut updated = true;
    let mut needs_reindex = false;

    match key {
        "usersOnLogin" => config.users_on_login = val.as_bool().unwrap_or(config.users_on_login),
        "enableGuest" => config.enable_guest = val.as_bool().unwrap_or(config.enable_guest),
        "enableWatchdog" => {
            config.enable_watchdog = val.as_bool().unwrap_or(config.enable_watchdog)
        }
        "enablePeriodicScans" => {
            config.enable_periodic_scans = val.as_bool().unwrap_or(config.enable_periodic_scans)
        }
        "rootDirs" => {
            if let Some(arr) = val.as_array() {
                config.root_dirs = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            } else {
                updated = false;
            }
        }
        "artistSeparators" => {
            if let Some(arr) = val.as_array() {
                config.artist_separators = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                needs_reindex = true;
            } else {
                updated = false;
            }
        }
        "artistSplitIgnoreList" => {
            if let Some(arr) = val.as_array() {
                config.artist_split_ignore_list = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect();
                needs_reindex = true;
            } else {
                updated = false;
            }
        }
        "removeProdBy" => {
            config.remove_prod_by = val.as_bool().unwrap_or(config.remove_prod_by);
            needs_reindex = true;
        }
        "removeRemasterInfo" => {
            config.remove_remaster_info = val.as_bool().unwrap_or(config.remove_remaster_info);
            needs_reindex = true;
        }
        "mergeAlbums" => {
            config.merge_albums = val.as_bool().unwrap_or(config.merge_albums);
            needs_reindex = true;
        }
        "cleanAlbumTitle" => {
            config.clean_album_title = val.as_bool().unwrap_or(config.clean_album_title);
            needs_reindex = true;
        }
        "showAlbumsAsSingles" => {
            config.show_albums_as_singles = val.as_bool().unwrap_or(config.show_albums_as_singles);
            needs_reindex = true;
        }
        _ => {
            updated = false;
        }
    }

    if !updated {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Unsupported setting key"
        }));
    }

    if let Err(_) = config.save() {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to save config"
        }));
    }

    if needs_reindex {
        spawn_library_scan(config, true);
    }

    HttpResponse::Ok().json(serde_json::json!({
        "msg": "Config updated!"
    }))
}

pub fn configure_upstream(cfg: &mut web::ServiceConfig) {
    cfg.service(add_root_dirs)
        .service(get_root_dirs_upstream)
        .service(get_all_settings_upstream)
        .service(trigger_scan_upstream)
        .service(update_config_upstream);
}

// ---------- Scan helpers ----------

#[derive(Debug)]
struct ScanStats {
    added: usize,
    updated: usize,
    removed: usize,
    total: usize,
}

fn spawn_library_scan(config: UserConfig, force: bool) {
    actix_web::rt::spawn(async move {
        match run_library_scan(config, force).await {
            Ok(stats) => info!(
                "Library scan completed (added: {}, updated: {}, removed: {}, total: {})",
                stats.added, stats.updated, stats.removed, stats.total
            ),
            Err(e) => error!("Library scan failed: {}", e),
        }
    });
}

async fn run_library_scan(config: UserConfig, force: bool) -> anyhow::Result<ScanStats> {
    use anyhow::anyhow;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use crate::core::images::{
        cache_album_images, download_artist_images, extract_album_colors, extract_artist_colors,
    };
    use crate::core::indexer::Indexer;
    use crate::core::mapstuff::{map_colors, map_favorites, map_scrobble_data};
    use crate::db::tables::TrackTable;
    use crate::stores::{AlbumStore, ArtistStore, FolderStore, TrackStore};
    use crate::utils::filesystem::normalize_path;

    let home_dir = directories::UserDirs::new()
        .and_then(|u| Some(u.home_dir().to_path_buf()))
        .map(|p| normalize_path(&p.to_string_lossy()));

    let root_dirs: Vec<String> = config
        .root_dirs
        .iter()
        .filter_map(|d| {
            if d == "$home" {
                home_dir.clone()
            } else {
                Some(normalize_path(d))
            }
        })
        .collect();

    if root_dirs.is_empty() {
        return Err(anyhow!("No root directories configured"));
    }

    let artist_seps = config.artist_separators.iter().cloned().collect();
    let indexer = Indexer::new(root_dirs, artist_seps).with_progress(false);

    // Scan filesystem
    let scanned_paths: Vec<PathBuf> = indexer.scan_files();
    let mut seen_norm: HashSet<String> = HashSet::new();

    // Existing tracks keyed by normalized path -> (raw path, track)
    let existing_tracks = TrackTable::all().await?;
    let mut existing_by_norm: HashMap<String, (String, crate::models::Track)> = HashMap::new();
    for track in existing_tracks {
        let norm = normalize_path(&track.filepath);
        existing_by_norm.insert(norm, (track.filepath.clone(), track));
    }

    let mut to_reindex: Vec<PathBuf> = Vec::new();

    for path in &scanned_paths {
        let norm = normalize_path(&path.to_string_lossy());
        seen_norm.insert(norm.clone());

        let file_mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let needs_reindex = if force {
            true
        } else {
            match existing_by_norm.get(&norm) {
                Some((_, existing)) => existing.last_mod != file_mtime,
                None => true,
            }
        };

        if needs_reindex {
            to_reindex.push(path.clone());
        }
    }

    // Paths removed from disk
    let removed_paths: Vec<String> = existing_by_norm
        .iter()
        .filter(|(norm, _)| !seen_norm.contains(*norm))
        .map(|(_, (raw, _))| raw.clone())
        .collect();

    if !removed_paths.is_empty() {
        let removed_count = TrackTable::remove_by_filepaths(&removed_paths).await?;
        info!("Removed {} missing tracks from database", removed_count);
    }

    // Reindex changed/new files
    let mut reindexed_tracks = indexer.reindex_files(&to_reindex)?;
    let mut updated_paths: Vec<String> = Vec::new();
    let mut added = 0usize;

    for track in &mut reindexed_tracks {
        let norm = normalize_path(&track.filepath);
        if let Some((raw, existing)) = existing_by_norm.get(&norm) {
            // Preserve play stats
            track.lastplayed = existing.lastplayed;
            track.playcount = existing.playcount;
            track.playduration = existing.playduration;
            updated_paths.push(raw.clone());
        } else {
            added += 1;
        }
    }

    if !updated_paths.is_empty() {
        TrackTable::remove_by_filepaths(&updated_paths).await?;
    }

    if !reindexed_tracks.is_empty() {
        TrackTable::insert_many(&reindexed_tracks).await?;
    }

    // Reload in-memory stores and mappings (parity with startup)
    TrackStore::load_all_tracks().await?;
    AlbumStore::load_albums().await?;
    ArtistStore::load_artists().await?;
    FolderStore::load_filepaths().await?;
    let cached = cache_album_images().await.unwrap_or(0);
    if cached > 0 {
        info!("Cached {} album covers from embedded art", cached);
    }
    // Extract colors from thumbnails
    let _ = extract_album_colors().await;
    // Download artist images and extract colors
    let _ = download_artist_images().await;
    let _ = extract_artist_colors().await;
    map_favorites().await?;
    map_colors().await?;
    map_scrobble_data().await?;

    let total = match TrackTable::count().await {
        Ok(count) => count as usize,
        Err(e) => {
            warn!("Failed to count tracks after scan: {}", e);
            0
        }
    };

    Ok(ScanStats {
        added,
        updated: updated_paths.len(),
        removed: removed_paths.len(),
        total,
    })
}
async fn resolve_user_id(req: &HttpRequest) -> Option<i64> {
    // prefer access token cookie
    let token = if let Some(cookie) = req.cookie("access_token_cookie") {
        Some(cookie.value().to_string())
    } else {
        match req.headers().get("Authorization") {
            Some(header_value) => {
                let header_str = header_value.to_str().unwrap_or("").trim();
                if header_str.is_empty() {
                    None
                } else if let Some(rest) = header_str.strip_prefix("Bearer ") {
                    if rest.is_empty() {
                        None
                    } else {
                        Some(rest.to_string())
                    }
                } else {
                    Some(header_str.to_string())
                }
            }
            None => None,
        }
    }?;

    let config = UserConfig::load().ok()?;
    let claims = verify_jwt(&token, &config.server_id, Some("access")).ok()?;
    let user = UserTable::get_by_id(claims.sub.id)
        .await
        .ok()??;
    Some(user.id)
}
