//! Folder browsing API routes

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use actix_web::{get, post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::UserConfig;
use crate::core::FolderLib;
use crate::db::tables::{FavoriteTable, PlaylistTable, TrackTable};
use crate::models::FavoriteType;
use crate::stores::{FolderStore, TrackStore};
use crate::utils::filesystem::{normalize_path, SUPPORTED_EXTENSIONS};

const USER_ID: i64 = 0;

/// Folder response
#[derive(Debug, Serialize)]
pub struct FolderResponse {
    pub name: String,
    pub path: String,
    pub is_sym: bool,
    pub trackcount: i32,
}

/// Track response (simplified)
#[derive(Debug, Serialize)]
pub struct FolderTrackResponse {
    pub trackhash: String,
    pub title: String,
    pub artist: String,
    pub duration: i32,
}

/// Folder contents response
#[derive(Debug, Serialize)]
pub struct FolderContentsResponse {
    pub folder: Option<FolderResponse>,
    pub subfolders: Vec<FolderResponse>,
    pub tracks: Vec<FolderTrackResponse>,
    pub breadcrumbs: Vec<BreadcrumbItem>,
}

/// Breadcrumb item
#[derive(Debug, Serialize)]
pub struct BreadcrumbItem {
    pub name: String,
    pub path: String,
}

fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') || path.ends_with('\\') {
        normalize_path(path)
    } else {
        format!("{}/", normalize_path(path))
    }
}

fn path_is_symlink(path: &str) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn folder_entry_from_path(path: &str) -> Option<FolderResponse> {
    let trackcount = FolderLib::recursive_track_count(path) as i32;
    if trackcount <= 0 {
        return None;
    }

    let path_buf = PathBuf::from(path);
    let name = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    Some(FolderResponse {
        name,
        path: ensure_trailing_slash(path),
        is_sym: path_is_symlink(path),
        trackcount,
    })
}

fn get_folders_from_paths(paths: &[String]) -> Vec<FolderResponse> {
    let counts = FolderStore::get().count_tracks_containing_paths(paths);
    counts
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .filter_map(|(path, trackcount)| {
            let entry = folder_entry_from_path(&path)?;
            Some(FolderResponse {
                trackcount,
                ..entry
            })
        })
        .collect()
}

fn sort_folders_for_folder(folders: &mut [FolderResponse], key: &str, reverse: bool) {
    if key == "default" {
        return;
    }

    let comparator = |a: &FolderResponse, b: &FolderResponse| match key {
        "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        "trackcount" => a.trackcount.cmp(&b.trackcount),
        "lastmod" => {
            let lhs = std::fs::metadata(&a.path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let rhs = std::fs::metadata(&b.path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            lhs.cmp(&rhs)
        }
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    };

    folders.sort_by(|a, b| {
        if reverse {
            comparator(b, a)
        } else {
            comparator(a, b)
        }
    });
}

fn sort_tracks_for_folder(tracks: &mut [crate::models::Track], key: &str, reverse: bool) {
    if key == "default" {
        return;
    }

    let comparator = |a: &crate::models::Track, b: &crate::models::Track| match key {
        "album" => a.album.to_lowercase().cmp(&b.album.to_lowercase()),
        "albumartists" => a
            .albumartists
            .get(0)
            .map(|ar| ar.name.to_lowercase())
            .cmp(&b.albumartists.get(0).map(|ar| ar.name.to_lowercase())),
        "artists" => a
            .artists
            .get(0)
            .map(|ar| ar.name.to_lowercase())
            .cmp(&b.artists.get(0).map(|ar| ar.name.to_lowercase())),
        "bitrate" => a.bitrate.cmp(&b.bitrate),
        "date" => a.date.cmp(&b.date),
        "disc" => {
            let disc_cmp = a.disc.cmp(&b.disc);
            if disc_cmp == std::cmp::Ordering::Equal {
                a.track.cmp(&b.track)
            } else {
                disc_cmp
            }
        }
        "duration" => a.duration.cmp(&b.duration),
        "last_mod" => a.last_mod.cmp(&b.last_mod),
        "lastplayed" => a.lastplayed.cmp(&b.lastplayed),
        "playduration" => a.playduration.cmp(&b.playduration),
        "playcount" => a.playcount.cmp(&b.playcount),
        "title" => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        _ => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
    };

    tracks.sort_by(|a, b| {
        if reverse {
            comparator(b, a)
        } else {
            comparator(a, b)
        }
    });
}

fn serialize_track_for_folder(
    track: &crate::models::Track,
    remove_disc: bool,
) -> serde_json::Value {
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
        ]
        .into_iter()
        .map(String::from)
        .collect();

        if remove_disc {
            to_remove.insert("disc".to_string());
            to_remove.insert("track".to_string());
        }

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
            if let Some(serde_json::Value::Array(items)) = map.get_mut(key) {
                for artist in items {
                    if let Some(obj) = artist.as_object_mut() {
                        obj.remove("image");
                    }
                }
            }
        }

        map.insert(
            "is_favorite".to_string(),
            serde_json::Value::Bool(track.is_favorite(USER_ID)),
        );
    }

    value
}

fn normalize_path_str(path: &str) -> String {
    normalize_path(path)
}

#[derive(Debug, Serialize)]
struct FolderTreeResult {
    path: String,
    folders: Vec<FolderResponse>,
    tracks: Vec<serde_json::Value>,
    total: usize,
}

fn collect_files_and_dirs(
    path_str: &str,
    params: &FolderTreeRequest,
    skip_empty_folders: bool,
) -> FolderTreeResult {
    let path = PathBuf::from(path_str);

    if !path.exists() || !path.is_dir() {
        return FolderTreeResult {
            path: normalize_path_str(path_str),
            folders: Vec::new(),
            tracks: Vec::new(),
            total: 0,
        };
    }

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let name = entry
                .file_name()
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_default();

            if name.starts_with('$') || name.starts_with('.') {
                continue;
            }

            if entry_path.is_dir() {
                dirs.push(entry_path);
            } else if entry_path.is_file() {
                if let Some(ext) = entry_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_lowercase())
                {
                    if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                        files.push(entry_path);
                    }
                }
            }
        }
    }

    let mut files_with_mtime = Vec::new();
    for file in files {
        if let Ok(metadata) = file.metadata() {
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    files_with_mtime.push((file, duration.as_secs()));
                }
            }
        }
    }

    files_with_mtime.sort_by_key(|(_, mtime)| *mtime);

    let file_paths: Vec<String> = files_with_mtime
        .into_iter()
        .map(|(p, _)| normalize_path_str(&p.to_string_lossy()))
        .collect();

    let total = file_paths.len();
    let mut tracks: Vec<_> = {
        let store = TrackStore::get();
        file_paths
            .iter()
            .filter_map(|p| store.get_by_path(p))
            .collect()
    };

    sort_tracks_for_folder(&mut tracks, &params.sorttracksby, params.tracksort_reverse);

    let start = params.start.max(0) as usize;
    let limit = if params.limit < 0 {
        tracks.len().saturating_sub(start)
    } else {
        params.limit as usize
    };
    let end = tracks.len().min(start.saturating_add(limit));

    let selected_tracks = if start < tracks.len() {
        tracks[start..end].to_vec()
    } else {
        Vec::new()
    };

    let serialized_tracks: Vec<_> = selected_tracks
        .iter()
        .map(|t| serialize_track_for_folder(t, true))
        .collect();

    let mut folder_entries: Vec<FolderResponse> = if params.tracks_only {
        Vec::new()
    } else {
        dirs.into_iter()
            .filter_map(|dir| folder_entry_from_path(&normalize_path_str(&dir.to_string_lossy())))
            .collect()
    };

    sort_folders_for_folder(
        &mut folder_entries,
        &params.sortfoldersby,
        params.foldersort_reverse,
    );

    if skip_empty_folders
        && !params.tracks_only
        && folder_entries.len() == 1
        && serialized_tracks.is_empty()
    {
        return collect_files_and_dirs(&folder_entries[0].path, params, true);
    }

    FolderTreeResult {
        path: ensure_trailing_slash(&normalize_path_str(path_str)),
        folders: folder_entries,
        tracks: serialized_tracks,
        total,
    }
}

fn get_all_drives(is_win: bool) -> Vec<String> {
    let mut drives = Vec::new();

    if is_win {
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            if Path::new(&drive).exists() {
                drives.push(normalize_path_str(&drive));
            }
        }
    } else {
        let root = Path::new("/");
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let path_str = normalize_path_str(&path.to_string_lossy());
                    let skip_prefixes = [
                        "/boot", "/tmp", "/snap", "/var", "/sys", "/proc", "/etc", "/run", "/dev",
                    ];

                    if skip_prefixes.iter().any(|p| path_str.starts_with(p)) {
                        continue;
                    }

                    drives.push(path_str);
                }
            }
        }

        if !drives.iter().any(|d| d == "/") {
            drives.insert(0, "/".to_string());
        }
    }

    drives.sort();
    drives.dedup();
    drives
}

/// Request for upstream-compatible folder tree
#[derive(Debug, Deserialize)]
pub struct FolderTreeRequest {
    #[serde(default = "default_folder_path")]
    pub folder: String,
    #[serde(default = "default_sorttracksby")]
    pub sorttracksby: String,
    #[serde(default)]
    pub tracksort_reverse: bool,
    #[serde(default = "default_sortfoldersby")]
    pub sortfoldersby: String,
    #[serde(default)]
    pub foldersort_reverse: bool,
    #[serde(default)]
    pub start: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub tracks_only: bool,
}

fn default_folder_path() -> String {
    "$home".to_string()
}

fn default_sorttracksby() -> String {
    "default".to_string()
}

fn default_sortfoldersby() -> String {
    "lastmod".to_string()
}

fn default_limit() -> i64 {
    50
}

/// Request for dir-browser (root selection)
#[derive(Debug, Deserialize)]
pub struct DirBrowserRequest {
    #[serde(default = "default_root_dir")]
    pub folder: String,
}

fn default_root_dir() -> String {
    "$root".to_string()
}

/// Query for opening folder in file manager
#[derive(Debug, Deserialize)]
pub struct OpenInFilesQuery {
    pub path: String,
}

/// Query for fetching tracks recursively
#[derive(Debug, Deserialize)]
pub struct TracksInPathQuery {
    pub path: String,
}

/// Query parameters for folder
#[derive(Debug, Deserialize)]
pub struct FolderQuery {
    pub path: Option<String>,
}

/// Get root directories
#[get("/roots")]
pub async fn get_roots() -> impl Responder {
    let roots = FolderLib::get_root_dirs();

    let folders: Vec<_> = roots
        .iter()
        .filter_map(|path| FolderLib::get_by_path(path))
        .map(|f| FolderResponse {
            name: f.name,
            path: f.path,
            is_sym: f.is_sym,
            trackcount: f.trackcount,
        })
        .collect();

    HttpResponse::Ok().json(folders)
}

/// Get folder contents
#[get("")]
pub async fn get_folder(query: web::Query<FolderQuery>) -> impl Responder {
    let path = match &query.path {
        Some(p) => p.clone(),
        None => {
            // Return roots if no path specified
            let roots = FolderLib::get_root_dirs();
            return HttpResponse::Ok().json(FolderContentsResponse {
                folder: None,
                subfolders: roots
                    .iter()
                    .filter_map(|p| FolderLib::get_by_path(p))
                    .map(|f| FolderResponse {
                        name: f.name,
                        path: f.path,
                        is_sym: f.is_sym,
                        trackcount: f.trackcount,
                    })
                    .collect(),
                tracks: Vec::new(),
                breadcrumbs: Vec::new(),
            });
        }
    };

    // Validate path is within root dirs
    if !FolderLib::is_valid_path(&path) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Path is not within configured root directories"
        }));
    }

    // Get folder info
    let folder = FolderLib::get_by_path(&path).map(|f| FolderResponse {
        name: f.name,
        path: f.path,
        is_sym: f.is_sym,
        trackcount: f.trackcount,
    });

    // Get subfolders
    let subfolders: Vec<_> = FolderLib::get_subfolders(&path)
        .into_iter()
        .map(|f| FolderResponse {
            name: f.name,
            path: f.path,
            is_sym: f.is_sym,
            trackcount: f.trackcount,
        })
        .collect();

    // Get tracks
    let tracks: Vec<_> = FolderLib::get_tracks(&path)
        .into_iter()
        .map(|t| FolderTrackResponse {
            trackhash: t.trackhash.clone(),
            title: t.title.clone(),
            artist: t.artist(),
            duration: t.duration,
        })
        .collect();

    // Get breadcrumbs
    let breadcrumbs: Vec<_> = FolderLib::get_breadcrumbs(&path)
        .into_iter()
        .map(|(name, path)| BreadcrumbItem { name, path })
        .collect();

    HttpResponse::Ok().json(FolderContentsResponse {
        folder,
        subfolders,
        tracks,
        breadcrumbs,
    })
}

/// Upstream-compatible folder tree (POST /folder)
#[post("")]
pub async fn get_folder_tree(body: web::Json<FolderTreeRequest>) -> impl Responder {
    let mut params = body.into_inner();
    let og_req_dir = params.folder.clone();
    let config = UserConfig::load().unwrap_or_default();
    let root_dirs = config.root_dirs.clone();

    if params.folder == "$home" && root_dirs.iter().any(|r| r == "$home") {
        if let Some(home) = directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()) {
            params.folder = normalize_path_str(&home.to_string_lossy());
        }
    }

    if params.folder == "$home" {
        let folders = get_folders_from_paths(&root_dirs);
        return HttpResponse::Ok().json(json!({
            "folders": folders,
            "tracks": Vec::<serde_json::Value>::new(),
        }));
    }

    if params.folder.starts_with("$playlist") {
        let parts: Vec<&str> = params.folder.split('/').collect();
        if parts.len() == 2 && !parts[1].is_empty() {
            let playlist_id: i64 = parts[1].parse().unwrap_or_default();
            match PlaylistTable::get_by_id(playlist_id).await {
                Ok(Some(playlist)) => {
                    let start = params.start.max(0) as usize;
                    let limit = if params.limit < 0 {
                        playlist.trackhashes.len().saturating_sub(start)
                    } else {
                        params.limit as usize
                    };

                    let end = playlist.trackhashes.len().min(start.saturating_add(limit));
                    let selected_hashes: Vec<String> = if start < playlist.trackhashes.len() {
                        playlist.trackhashes[start..end].to_vec()
                    } else {
                        Vec::new()
                    };

                    let tracks = TrackStore::get().get_by_hashes(&selected_hashes);
                    let serialized: Vec<_> = tracks
                        .iter()
                        .map(|t| serialize_track_for_folder(t, true))
                        .collect();

                    return HttpResponse::Ok().json(json!({
                        "path": format!("$playlist/{}", playlist.name),
                        "folders": Vec::<FolderResponse>::new(),
                        "tracks": serialized,
                    }));
                }
                _ => {
                    return HttpResponse::Ok().json(json!({
                        "path": params.folder,
                        "folders": Vec::<FolderResponse>::new(),
                        "tracks": Vec::<serde_json::Value>::new(),
                    }));
                }
            }
        }

        let mut playlists = PlaylistTable::all(None).await.unwrap_or_default();
        playlists.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
        let folders: Vec<_> = playlists
            .into_iter()
            .map(|p| FolderResponse {
                name: p.name,
                path: format!("$playlist/{}", p.id),
                is_sym: false,
                trackcount: p.count,
            })
            .collect();

        return HttpResponse::Ok().json(json!({
            "path": params.folder,
            "folders": folders,
            "tracks": Vec::<serde_json::Value>::new(),
        }));
    }

    if params.folder == "$favorites" {
        let limit = if params.limit < 0 {
            i64::MAX / 4
        } else {
            params.limit
        };
        let favorites =
            FavoriteTable::get_by_type(FavoriteType::Track, USER_ID, params.start, limit)
                .await
                .unwrap_or_default();

        let trackhashes: Vec<String> = favorites.into_iter().map(|f| f.hash).collect();
        let tracks = TrackStore::get().get_by_hashes(&trackhashes);
        let serialized: Vec<_> = tracks
            .iter()
            .map(|t| serialize_track_for_folder(t, true))
            .collect();

        return HttpResponse::Ok().json(json!({
            "tracks": serialized,
            "folders": Vec::<FolderResponse>::new(),
            "path": params.folder,
        }));
    }

    if !Path::new(&params.folder).exists() {
        let patched = format!("/{}", params.folder.trim_start_matches('/'));
        if Path::new(&patched).exists() {
            params.folder = patched;
        }
    }

    let mut result = collect_files_and_dirs(&params.folder, &params, true);

    if og_req_dir == "$home" && config.show_playlists_in_folder_view {
        let favorites_item = FolderResponse {
            name: "Favorites".to_string(),
            path: "$favorites".to_string(),
            is_sym: false,
            trackcount: FavoriteTable::count_tracks(USER_ID).await.unwrap_or(0) as i32,
        };

        let playlists = PlaylistTable::all(None).await.unwrap_or_default();
        let playlist_sum: i32 = playlists.iter().map(|p| p.count).sum();

        let playlists_item = FolderResponse {
            name: "Playlists".to_string(),
            path: "$playlists".to_string(),
            is_sym: false,
            trackcount: playlist_sum,
        };

        result.folders.insert(0, playlists_item);
        result.folders.insert(0, favorites_item);
    }

    HttpResponse::Ok().json(result)
}

/// Get parent folder
#[get("/parent")]
pub async fn get_parent(query: web::Query<FolderQuery>) -> impl Responder {
    let path = match &query.path {
        Some(p) => p,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Path is required"
            }));
        }
    };

    match FolderLib::get_parent(path) {
        Some(parent) => {
            if FolderLib::is_valid_path(&parent) {
                HttpResponse::Ok().json(serde_json::json!({
                    "path": parent
                }))
            } else {
                HttpResponse::Ok().json(serde_json::json!({
                    "path": null
                }))
            }
        }
        None => HttpResponse::Ok().json(serde_json::json!({
            "path": null
        })),
    }
}

/// List folders for root selection
#[post("/dir-browser")]
pub async fn list_folders(body: web::Json<DirBrowserRequest>) -> impl Responder {
    let req_dir = body.folder.clone();
    let is_win = cfg!(windows);

    if req_dir == "$root" {
        let folders: Vec<_> = get_all_drives(is_win)
            .into_iter()
            .map(|p| json!({ "name": p.clone(), "path": p }))
            .collect();
        return HttpResponse::Ok().json(json!({ "folders": folders }));
    }

    let mut dir_path = PathBuf::from(&req_dir);
    if !dir_path.exists() {
        let patched = PathBuf::from(format!("/{}", req_dir.trim_start_matches('/')));
        if patched.exists() {
            dir_path = patched;
        }
    }

    let mut folders = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry
                .file_name()
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_default();

            if name.starts_with('$') || name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                folders.push(json!({
                    "name": name,
                    "path": normalize_path_str(&path.to_string_lossy()),
                }));
            }
        }
    }

    folders.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });

    HttpResponse::Ok().json(json!({ "folders": folders }))
}

/// Open path in file manager (no-op placeholder)
#[get("/show-in-files")]
pub async fn open_in_file_manager(_query: web::Query<OpenInFilesQuery>) -> impl Responder {
    HttpResponse::Ok().json(json!({ "success": true }))
}

/// Get tracks in a path recursively (max 300)
#[get("/tracks/all")]
pub async fn get_tracks_in_path(query: web::Query<TracksInPathQuery>) -> impl Responder {
    let path_prefix = normalize_path_str(&query.path);
    let mut tracks = TrackTable::get_by_folder_containing(&path_prefix)
        .await
        .unwrap_or_default();

    // limit to 300 like upstream
    tracks.truncate(300);

    let serialized: Vec<_> = tracks
        .iter()
        .map(|t| serialize_track_for_folder(t, true))
        .collect();

    HttpResponse::Ok().json(json!({ "tracks": serialized }))
}

/// Configure folder routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_roots)
        .service(get_folder)
        .service(get_folder_tree)
        .service(list_folders)
        .service(open_in_file_manager)
        .service(get_tracks_in_path)
        .service(get_parent);
}
