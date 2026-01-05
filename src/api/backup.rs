//! Backup and restore API routes aligned with Python upstream behavior

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use actix_web::{delete, get, post, web, HttpResponse, Responder};
use directories::UserDirs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::config::Paths;
use crate::db::tables::{CollectionTable, FavoriteTable, PlaylistTable, ScrobbleTable};
use crate::models::{Favorite, Playlist, TrackLog};
use crate::utils::dates::timestamp_to_relative;

const USER_ID: i64 = 0;

#[derive(Debug, Serialize)]
struct BackupCreateResponse {
    name: String,
    date: String,
    scrobbles: usize,
    favorites: usize,
    playlists: usize,
    collections: usize,
}

#[derive(Debug, Deserialize)]
struct RestoreBackupBody {
    #[serde(default)]
    backup_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteBackupBody {
    backup_dir: String,
}

#[derive(Debug, Serialize)]
struct BackupListItem {
    name: String,
    date: String,
    scrobbles: usize,
    favorites: usize,
    playlists: usize,
    collections: usize,
}

#[post("/create")]
pub async fn create_backup() -> impl Responder {
    let backup_root = backup_root();
    if let Err(e) = fs::create_dir_all(&backup_root) {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    let backup_name = format!("backup.{}", chrono::Utc::now().timestamp());
    let backup_dir = backup_root.join(&backup_name);
    let backup_file = backup_dir.join("data.json");
    let img_folder = backup_dir.join("images");

    if let Err(e) = fs::create_dir_all(&backup_dir) {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    // Favorites
    let favorites: Vec<Favorite> = match FavoriteTable::all(Some(USER_ID)).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
    };

    let favorites_json: Vec<Value> = favorites
        .iter()
        .filter_map(|f| serde_json::to_value(f).ok())
        .collect();

    // Scrobbles
    let scrobbles: Vec<TrackLog> = match ScrobbleTable::get_all().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
    };

    let mut scrobbles_json: Vec<Map<String, Value>> = scrobbles
        .iter()
        .filter_map(|s| serde_json::to_value(s).ok())
        .filter_map(|v| v.as_object().cloned())
        .collect();
    for scrobble in scrobbles_json.iter_mut() {
        scrobble.remove("id");
    }

    // Playlists
    let playlists: Vec<Playlist> = match PlaylistTable::all(Some(USER_ID)).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
    };

    let mut playlist_dicts: Vec<Map<String, Value>> = Vec::new();
    let mut img_folder_created = img_folder.exists();
    let playlist_img_dir = Paths::get()
        .map(|p| p.playlist_images_dir())
        .unwrap_or_else(|_| PathBuf::from(""));

    for playlist in playlists.iter() {
        let mut map = serde_json::to_value(playlist)
            .unwrap_or_else(|_| json!({}))
            .as_object()
            .cloned()
            .unwrap_or_default();
        for key in [
            "id",
            "_last_updated",
            "has_image",
            "images",
            "duration",
            "count",
            "pinned",
            "thumb",
        ] {
            map.remove(key);
        }

        if let Some(img) = map.get("image").and_then(|v| v.as_str()) {
            let src = playlist_img_dir.join(img);
            if src.exists() {
                if !img_folder_created {
                    if fs::create_dir_all(&img_folder).is_ok() {
                        img_folder_created = true;
                    }
                }
                let _ = fs::copy(&src, img_folder.join(img));
            }
        }

        playlist_dicts.push(map);
    }

    // Collections
    let collections_rows = match CollectionTable::get_all().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
    };

    let mut collections_json: Vec<Map<String, Value>> = Vec::new();
    for collection in collections_rows {
        let items_val: Value = serde_json::from_str(&collection.settings).unwrap_or(json!([]));
        let extra_val: Value = collection
            .extra_data
            .and_then(|e| serde_json::from_str(&e).ok())
            .unwrap_or_else(|| json!({}));

        let mut map = Map::new();
        map.insert("name".to_string(), Value::String(collection.name));
        map.insert("items".to_string(), items_val);
        map.insert("extra".to_string(), extra_val);
        map.insert("userid".to_string(), Value::Number(0u64.into()));
        collections_json.push(map);
    }

    let data = json!({
        "favorites": favorites_json,
        "scrobbles": scrobbles_json,
        "playlists": playlist_dicts,
        "collections": collections_json,
    });

    if let Err(e) = fs::create_dir_all(backup_file.parent().unwrap_or_else(|| Path::new("."))) {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    let content = match serde_json::to_string_pretty(&data) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
    };

    if let Err(e) = fs::write(&backup_file, content) {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    let ts = backup_name
        .split('.')
        .nth(1)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    HttpResponse::Ok().json(BackupCreateResponse {
        name: backup_name.clone(),
        date: timestamp_to_relative(ts),
        scrobbles: scrobbles_json.len(),
        favorites: favorites_json.len(),
        playlists: playlist_dicts.len(),
        collections: collections_json.len(),
    })
}

#[post("/restore")]
pub async fn restore_backup(body: web::Json<RestoreBackupBody>) -> impl Responder {
    let backup_root = backup_root();
    let mut restored: Vec<String> = Vec::new();

    if let Some(dir) = &body.backup_dir {
        let target = backup_root.join(dir);
        if !target.exists() || !target.is_dir() {
            return HttpResponse::NotFound()
                .json(json!({"msg": format!("Backup '{}' not found", dir)}));
        }

        if let Err(e) = restore_from_dir(&target).await {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
        restored.push(dir.clone());
    } else {
        let dirs = fs::read_dir(&backup_root)
            .ok()
            .into_iter()
            .flat_map(|it| it.filter_map(|e| e.ok()))
            .filter(|e| e.path().is_dir())
            .collect::<Vec<_>>();

        if dirs.is_empty() {
            return HttpResponse::NotFound().json(json!({"msg": "No backups found"}));
        }

        let mut entries: Vec<PathBuf> = dirs.into_iter().map(|d| d.path()).collect();
        entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        for dir in entries {
            if let Err(e) = restore_from_dir(&dir).await {
                eprintln!("{}", e);
                return HttpResponse::InternalServerError()
                    .json(json!({"msg": "Failed! An error occured"}));
            }
            if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                restored.push(name.to_string());
            }
        }
    }

    // Map favorites/scrobbles into stores for parity with upstream index_everything
    let _ = crate::core::mapstuff::map_favorites().await;
    let _ = crate::core::mapstuff::map_scrobble_data().await;

    HttpResponse::Ok().json(json!({"msg": "Restored successfully", "backups": restored}))
}

#[get("/list")]
pub async fn list_backups() -> impl Responder {
    let backup_root = backup_root();
    let mut entries: Vec<(PathBuf, i64)> = Vec::new();

    if let Ok(paths) = fs::read_dir(&backup_root) {
        for entry in paths.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(ts) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|s| s.split('.').nth(1))
                    .and_then(|t| t.parse::<i64>().ok())
                {
                    entries.push((path, ts));
                }
            }
        }
    }

    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let mut backups: Vec<BackupListItem> = Vec::new();
    for (path, ts) in entries {
        let mut info = BackupListItem {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            date: timestamp_to_relative(ts),
            scrobbles: 0,
            favorites: 0,
            playlists: 0,
            collections: 0,
        };

        let json_file = path.join("data.json");
        if let Ok(file) = fs::File::open(&json_file) {
            if let Ok(data) = serde_json::from_reader::<_, Value>(file) {
                info.scrobbles = data
                    .get("scrobbles")
                    .and_then(|v| v.as_array())
                    .map(|v| v.len())
                    .unwrap_or(0);
                info.favorites = data
                    .get("favorites")
                    .and_then(|v| v.as_array())
                    .map(|v| v.len())
                    .unwrap_or(0);
                info.playlists = data
                    .get("playlists")
                    .and_then(|v| v.as_array())
                    .map(|v| v.len())
                    .unwrap_or(0);
                info.collections = data
                    .get("collections")
                    .and_then(|v| v.as_array())
                    .map(|v| v.len())
                    .unwrap_or(0);
            }
        }

        backups.push(info);
    }

    HttpResponse::Ok().json(json!({"backups": backups}))
}

#[delete("/delete")]
pub async fn delete_backup(body: web::Json<DeleteBackupBody>) -> impl Responder {
    let backup_root = backup_root();
    let target = backup_root.join(&body.backup_dir);

    if !target.exists() || !target.is_dir() {
        return HttpResponse::NotFound()
            .json(json!({"msg": format!("Backup '{}' not found", body.backup_dir)}));
    }

    if let Err(e) = fs::remove_dir_all(&target) {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    HttpResponse::Ok().json(json!({"msg": format!("Backup '{}' deleted", body.backup_dir)}))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_backup)
        .service(restore_backup)
        .service(list_backups)
        .service(delete_backup);
}

fn backup_root() -> PathBuf {
    UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("swingmusic.backup")
}

async fn restore_from_dir(dir: &Path) -> anyhow::Result<()> {
    let data_file = dir.join("data.json");
    let file = fs::File::open(&data_file)?;
    let data: Value = serde_json::from_reader(file)?;

    restore_favorites(data.get("favorites").cloned().unwrap_or(json!([]))).await?;
    restore_playlists(dir, data.get("playlists").cloned().unwrap_or(json!([]))).await?;
    restore_scrobbles(data.get("scrobbles").cloned().unwrap_or(json!([]))).await?;
    restore_collections(data.get("collections").cloned().unwrap_or(json!([]))).await?;

    Ok(())
}

async fn restore_favorites(favs: Value) -> anyhow::Result<()> {
    let favorites: Vec<Favorite> = serde_json::from_value(favs).unwrap_or_default();
    let mut existing: HashSet<(String, String)> = FavoriteTable::all(Some(USER_ID))
        .await?
        .into_iter()
        .map(|f| (f.favorite_type.as_str().to_string(), f.hash.clone()))
        .collect();

    for fav in favorites {
        let key = (fav.favorite_type.as_str().to_string(), fav.hash.clone());
        if existing.contains(&key) {
            continue;
        }

        if let Err(e) =
            FavoriteTable::add_with_extra(&fav.hash, fav.favorite_type, USER_ID, &fav.extra).await
        {
            eprintln!("{}", e);
        } else {
            existing.insert(key);
        }
    }

    Ok(())
}

async fn restore_playlists(dir: &Path, playlists: Value) -> anyhow::Result<()> {
    let playlists: Vec<Map<String, Value>> = playlists
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();

    let existing: HashSet<String> = PlaylistTable::all(Some(USER_ID))
        .await?
        .into_iter()
        .map(|p| p.name)
        .collect();

    let mut playlist_names = existing;
    let paths = Paths::get().ok();

    for mut map in playlists {
        if let Some(name) = map
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            if playlist_names.contains(&name) {
                continue;
            }
            map.remove("_score");

            let playlist: Playlist = serde_json::from_value(Value::Object(map.clone()))
                .unwrap_or_else(|_| Playlist::new(name.clone(), Some(USER_ID)));

            if let Err(e) = PlaylistTable::insert(&playlist).await {
                eprintln!("{}", e);
                continue;
            }

            if let (Some(paths), Some(img)) =
                (paths.as_ref(), map.get("image").and_then(|v| v.as_str()))
            {
                let src = dir.join("images").join(img);
                let dest = paths.playlist_images_dir().join(img);
                if src.exists() {
                    let _ = fs::create_dir_all(dest.parent().unwrap_or_else(|| Path::new(".")));
                    let _ = fs::copy(&src, &dest);
                }
            }

            playlist_names.insert(name);
        }
    }

    Ok(())
}

async fn restore_scrobbles(scrobbles: Value) -> anyhow::Result<()> {
    let scrobbles: Vec<Map<String, Value>> = scrobbles
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();

    let existing_logs = ScrobbleTable::get_all().await.unwrap_or_default();
    let mut existing_keys: HashSet<String> = existing_logs
        .iter()
        .map(|s| format!("{}.{}", s.trackhash, s.timestamp))
        .collect();

    for scrobble in scrobbles {
        if let (Some(trackhash), Some(timestamp)) = (
            scrobble.get("trackhash").and_then(|v| v.as_str()),
            scrobble.get("timestamp").and_then(|v| v.as_i64()),
        ) {
            let key = format!("{}.{}", trackhash, timestamp);
            if existing_keys.contains(&key) {
                continue;
            }

            let duration = scrobble
                .get("duration")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            let source = scrobble
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let userid = scrobble
                .get("userid")
                .and_then(|v| v.as_i64())
                .unwrap_or(USER_ID);
            let extra = scrobble.get("extra").cloned().unwrap_or(json!({}));

            if let Err(e) = ScrobbleTable::add_with_extra(
                trackhash, timestamp, duration, source, userid, &extra,
            )
            .await
            {
                eprintln!("{}", e);
            } else {
                existing_keys.insert(key);
            }
        }
    }

    Ok(())
}

async fn restore_collections(collections: Value) -> anyhow::Result<()> {
    let collections: Vec<Map<String, Value>> = collections
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();

    let existing = CollectionTable::get_all().await.unwrap_or_default();
    let mut names: HashSet<String> = existing.into_iter().map(|c| c.name).collect();

    for collection in collections {
        if let Some(name) = collection.get("name").and_then(|v| v.as_str()) {
            if names.contains(name) {
                continue;
            }

            let items_val = collection
                .get("items")
                .cloned()
                .or_else(|| collection.get("settings").cloned())
                .unwrap_or_else(|| json!([]));
            let settings_str =
                serde_json::to_string(&items_val).unwrap_or_else(|_| "[]".to_string());
            let extra_val = collection
                .get("extra")
                .cloned()
                .or_else(|| collection.get("extra_data").cloned())
                .unwrap_or_else(|| json!({}));
            let extra_str = serde_json::to_string(&extra_val).unwrap_or_else(|_| "{}".to_string());

            if let Err(e) = CollectionTable::insert(name, &settings_str, Some(&extra_str)).await {
                eprintln!("{}", e);
                continue;
            }

            names.insert(name.to_string());
        }
    }

    Ok(())
}
