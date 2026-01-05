//! Playlist API routes (aligned with upstream Flask `/playlists` endpoints)

use actix_multipart::Multipart;
use actix_web::{delete, get, post, put, web, HttpResponse, Responder};
use futures::StreamExt;
use image::imageops::FilterType;
use image::{GenericImageView, ImageFormat};
use serde::Deserialize;
use std::fs;
use std::io::Write;

use crate::config::Paths;
use crate::core::PlaylistLib;
use crate::db::tables::PlaylistTable;
use crate::models::Playlist;
use crate::stores::{AlbumStore, TrackStore};
use crate::utils::auth::generate_random_string;
use crate::utils::dates::date_to_relative;

#[derive(Debug, Deserialize)]
pub struct SendAllQuery {
    #[serde(default)]
    pub no_images: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreatePlaylistBody {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveAsPlaylistBody {
    pub itemtype: String,
    pub playlist_name: String,
    pub itemhash: String,
    #[serde(default)]
    pub sortoptions: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddItemBody {
    #[serde(default = "default_itemtype")]
    pub itemtype: String,
    pub itemhash: String,
    #[serde(default)]
    pub sortoptions: Option<serde_json::Value>,
}

fn default_itemtype() -> String {
    "tracks".to_string()
}

#[derive(Debug, Deserialize)]
pub struct GetPlaylistQuery {
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub no_tracks: bool,
    #[serde(default)]
    pub start: usize,
}

#[derive(Debug, Deserialize)]
pub struct RemoveTracksBody {
    pub tracks: Vec<RemoveTrackItem>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveTrackItem {
    pub trackhash: String,
    pub index: usize,
}

/// GET /playlists
#[get("")]
pub async fn send_all_playlists(query: web::Query<SendAllQuery>) -> impl Responder {
    let _ = query.no_images;
    let playlists = match PlaylistLib::get_all().await {
        Ok(p) => p,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get playlists"
            }))
        }
    };

    let mut playlists = playlists;
    playlists.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));

    let data: Vec<_> = playlists
        .into_iter()
        .map(|mut p| {
            p.init();
            let images = if !p.has_image {
                first_4_images(None, Some(&p.trackhashes))
            } else {
                Vec::new()
            };
            serialize_playlist(&p, &images)
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "data": data,
    }))
}

/// POST /playlists/new
#[post("/new")]
pub async fn create_playlist(body: web::Json<CreatePlaylistBody>) -> impl Responder {
    let userid = 1;
    match PlaylistTable::name_exists(&body.name, userid).await {
        Ok(true) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "error": "Playlist already exists"
            }))
        }
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Database error"
            }))
        }
        _ => {}
    }

    let playlist = Playlist::new(body.name.clone(), Some(userid));
    match PlaylistTable::insert(&playlist).await {
        Ok(_) => match PlaylistLib::get_all()
            .await
            .ok()
            .and_then(|mut list| list.pop())
        {
            Some(p) => HttpResponse::Created().json(serde_json::json!({ "playlist": p })),
            None => HttpResponse::Created().json(serde_json::json!({ "playlist": playlist })),
        },
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Playlist could not be created"
        })),
    }
}

/// POST /playlists/<playlistid>/add
#[post("/{playlistid}/add")]
pub async fn add_item_to_playlist(
    path: web::Path<String>,
    body: web::Json<AddItemBody>,
) -> impl Responder {
    let playlist_id: i64 = match path.parse() {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid playlist id"
            }))
        }
    };

    let trackhashes =
        resolve_item_trackhashes(&body.itemtype, &body.itemhash, body.sortoptions.as_ref());

    if body.itemtype == "tracks" {
        if trackhashes.len() == 1 {
            if let Ok(existing) = PlaylistTable::get_trackhashes(playlist_id).await {
                if existing.contains(&trackhashes[0]) {
                    return HttpResponse::Conflict().json(serde_json::json!({
                        "msg": "Track already exists in playlist"
                    }));
                }
            }
        }
    }

    if PlaylistTable::add_tracks(playlist_id, &trackhashes)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to add to playlist"
        }));
    }

    HttpResponse::Ok().json(serde_json::json!({ "msg": "Done" }))
}

/// GET /playlists/<playlistid>
#[get("/{playlistid}")]
pub async fn get_playlist(
    path: web::Path<String>,
    query: web::Query<GetPlaylistQuery>,
) -> impl Responder {
    let playlistid = path.into_inner();
    let mut limit = if query.limit == 0 { 6 } else { query.limit };

    if playlistid == "recentlyadded" || playlistid == "recentlyplayed" {
        if query.start != 0 {
            return HttpResponse::Ok().json(serde_json::json!({ "tracks": [] }));
        }
        let (playlist, tracks) = build_custom_playlist(&playlistid);
        let images = first_4_images(Some(&tracks), None);
        return HttpResponse::Ok().json(serde_json::json!({
            "info": serialize_playlist(&playlist, &images),
            "tracks": tracks.iter().map(|t| serialize_track_for_playlist(t)).collect::<Vec<_>>(),
        }));
    }

    let pid: i64 = match playlistid.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "msg": "Playlist not found"
            }))
        }
    };

    let mut playlist = match PlaylistTable::get_by_id(pid).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "msg": "Playlist not found"
            }))
        }
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Database error"
            }))
        }
    };

    let track_total = playlist.trackhashes.len();
    if limit == -1 {
        limit = track_total.saturating_sub(1) as i64;
    }

    let end = (query.start as i64 + limit) as usize;
    let slice: Vec<String> = playlist
        .trackhashes
        .iter()
        .skip(query.start)
        .take(end.saturating_sub(query.start))
        .cloned()
        .collect();

    let store = TrackStore::get();
    let tracks = store.get_by_hashes(&slice);
    let duration: i32 = tracks.iter().map(|t| t.duration).sum();

    playlist.duration = duration;
    playlist.count = tracks.len() as i32;
    playlist.images = Vec::new();
    playlist.last_updated = date_to_relative(&playlist.last_updated);
    playlist.init();

    let images = first_4_images(None, Some(&playlist.trackhashes));

    let serialized_tracks = if query.no_tracks {
        Vec::new()
    } else {
        tracks
            .iter()
            .map(|t| serialize_track_for_playlist(t))
            .collect()
    };

    HttpResponse::Ok().json(serde_json::json!({
        "info": serialize_playlist(&playlist, &images),
        "tracks": serialized_tracks,
    }))
}

/// PUT /playlists/<playlistid>/update
#[put("/{playlistid}/update")]
pub async fn update_playlist_info(
    path: web::Path<String>,
    mut payload: Multipart,
) -> impl Responder {
    let playlistid: i64 = match path.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({ "error": "Playlist not found" }))
        }
    };

    let mut playlist = match PlaylistTable::get_by_id(playlistid).await {
        Ok(Some(p)) => p,
        _ => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Playlist not found"
            }))
        }
    };

    let mut new_name: Option<String> = None;
    let mut settings_raw: Option<String> = None;
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_content_type: Option<String> = None;

    while let Some(Ok(mut field)) = payload.next().await {
        let disp = field.content_disposition().clone();
        let name = disp.get_name().map(|s| s.to_string()).unwrap_or_default();

        let mut bytes = Vec::new();
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(data) => bytes.extend_from_slice(&data),
                Err(_) => continue,
            }
        }

        match name.as_str() {
            "name" => {
                new_name = Some(String::from_utf8_lossy(&bytes).trim().to_string());
            }
            "settings" => {
                settings_raw = Some(String::from_utf8_lossy(&bytes).to_string());
            }
            "image" => {
                image_bytes = Some(bytes);
                image_content_type = field.content_type().map(|ct| ct.to_string());
            }
            _ => {}
        }
    }

    if let Some(n) = new_name {
        playlist.name = n;
    }

    if let Some(settings_str) = settings_raw {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&settings_str) {
            playlist.settings = serde_json::from_value(val).unwrap_or_default();
        }
    }

    let mut has_gif = false;
    if let Some(bytes) = image_bytes {
        match save_playlist_image(
            playlistid,
            &bytes,
            image_content_type.as_deref().unwrap_or("image/webp"),
            playlist.image.as_deref(),
        ) {
            Ok((filename, is_gif)) => {
                has_gif = is_gif;
                playlist.image = Some(filename);
                playlist.settings.has_gif = is_gif;
                playlist.has_image = true;
                playlist.thumb = playlist
                    .image
                    .as_ref()
                    .map(|f| format!("thumb_{}", f))
                    .unwrap_or_default();
            }
            Err(_) => {
                return HttpResponse::BadRequest()
                    .json(serde_json::json!({ "error": "Failed: Invalid image" }));
            }
        }
    }

    // gif flag should be false unless explicitly set by image type
    if !has_gif {
        playlist.settings.has_gif = false;
    }

    if PlaylistTable::update(&playlist).await.is_err() {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to update playlist"
        }));
    }

    playlist.last_updated = date_to_relative(&playlist.last_updated);
    playlist.init();
    playlist.clear_trackhashes();
    let images = if playlist.has_image {
        Vec::new()
    } else {
        first_4_images(None, Some(&playlist.trackhashes))
    };

    HttpResponse::Ok().json(serde_json::json!({ "data": serialize_playlist(&playlist, &images) }))
}

/// POST /playlists/<playlistid>/pin_unpin
#[post("/{playlistid}/pin_unpin")]
pub async fn pin_unpin_playlist(path: web::Path<String>) -> impl Responder {
    let playlistid: i64 = match path.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({ "error": "Playlist not found" }))
        }
    };

    let mut playlist = match PlaylistTable::get_by_id(playlistid).await {
        Ok(Some(p)) => p,
        _ => {
            return HttpResponse::NotFound()
                .json(serde_json::json!({ "error": "Playlist not found" }))
        }
    };

    playlist.settings.pinned = !playlist.settings.pinned;

    if PlaylistTable::update_settings(playlistid, &playlist.settings)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": "Failed to update" }));
    }

    HttpResponse::Ok().json(serde_json::json!({ "msg": "Done" }))
}

/// DELETE /playlists/<playlistid>/remove-img
#[delete("/{playlistid}/remove-img")]
pub async fn remove_playlist_image(path: web::Path<String>) -> impl Responder {
    let playlistid: i64 = match path.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({ "error": "Playlist not found" }))
        }
    };

    let mut playlist = match PlaylistTable::get_by_id(playlistid).await.ok().flatten() {
        Some(p) => p,
        None => return HttpResponse::Ok().json(serde_json::json!({ "msg": "Done" })),
    };

    if let Some(img) = &playlist.image {
        let _ = delete_playlist_images(img);
    }

    playlist.image = None;
    playlist.thumb.clear();
    playlist.settings.has_gif = false;
    playlist.has_image = false;

    if PlaylistTable::remove_image(playlistid).await.is_err() {
        return HttpResponse::InternalServerError().json(serde_json::json!({ "error": "Failed" }));
    }

    playlist.last_updated = date_to_relative(&playlist.last_updated);
    playlist.init();
    let images = first_4_images(None, Some(&playlist.trackhashes));

    HttpResponse::Ok()
        .json(serde_json::json!({ "playlist": serialize_playlist(&playlist, &images) }))
}

/// DELETE /playlists/<playlistid>/delete
#[delete("/{playlistid}/delete")]
pub async fn remove_playlist(path: web::Path<String>) -> impl Responder {
    let playlistid: i64 = match path.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({ "error": "Playlist not found" }))
        }
    };

    let user = PlaylistTable::get_by_id(playlistid)
        .await
        .ok()
        .flatten()
        .and_then(|p| p.userid)
        .unwrap_or(1);

    if PlaylistTable::delete(playlistid, user)
        .await
        .unwrap_or(false)
    {
        HttpResponse::Ok().json(serde_json::json!({ "msg": "Done" }))
    } else {
        HttpResponse::InternalServerError().json(serde_json::json!({ "error": "Failed" }))
    }
}

/// POST /playlists/<playlistid>/remove-tracks
#[post("/{playlistid}/remove-tracks")]
pub async fn remove_tracks_from_playlist(
    path: web::Path<String>,
    body: web::Json<RemoveTracksBody>,
) -> impl Responder {
    let playlistid: i64 = match path.parse() {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({ "error": "Playlist not found" }))
        }
    };

    let items: Vec<(usize, String)> = body
        .tracks
        .iter()
        .map(|t| (t.index, t.trackhash.clone()))
        .collect();

    if PlaylistTable::remove_tracks(playlistid, &items)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().json(serde_json::json!({ "error": "Failed" }));
    }

    HttpResponse::Ok().json(serde_json::json!({ "msg": "Done" }))
}

/// POST /playlists/save-item
#[post("/save-item")]
pub async fn save_item_as_playlist(body: web::Json<SaveAsPlaylistBody>) -> impl Responder {
    if PlaylistTable::name_exists(&body.playlist_name, 1)
        .await
        .unwrap_or(false)
    {
        return HttpResponse::Conflict()
            .json(serde_json::json!({ "error": "Playlist already exists" }));
    }

    let trackhashes =
        resolve_item_trackhashes(&body.itemtype, &body.itemhash, Some(&body.sortoptions));

    if trackhashes.is_empty() {
        return HttpResponse::NotFound().json(serde_json::json!({ "error": "No tracks founds" }));
    }

    let mut playlist = Playlist::new(body.playlist_name.clone(), Some(1));
    playlist.trackhashes = trackhashes.clone();
    playlist.count = trackhashes.len() as i32;

    let images = first_4_images(None, Some(&trackhashes));

    let id = match PlaylistTable::insert(&playlist).await {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({ "error": "Playlist could not be created" }))
        }
    };

    playlist.id = id;

    if body.itemtype != "folder" && body.itemtype != "tracks" {
        if let Some(img) = copy_source_image(id, &body.itemtype, &body.itemhash) {
            playlist.image = Some(img.clone());
            playlist.has_image = true;
            playlist.thumb = format!("thumb_{}", img);
        }
    }

    if PlaylistTable::update(&playlist).await.is_err() {
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": "Playlist could not be created" }));
    }

    HttpResponse::Created()
        .json(serde_json::json!({ "playlist": serialize_playlist(&playlist, &images) }))
}

fn resolve_item_trackhashes(
    itemtype: &str,
    itemhash: &str,
    sortoptions: Option<&serde_json::Value>,
) -> Vec<String> {
    let store = TrackStore::get();
    match itemtype {
        "tracks" => itemhash.split(',').map(|s| s.to_string()).collect(),
        "folder" => {
            let sortreverse = sortoptions
                .and_then(|v| v.get("tracksortreverse"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let sortby = sortoptions
                .and_then(|v| v.get("tracksortby"))
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            let mut tracks = store.get_by_folder(itemhash);
            sort_tracks_py(&mut tracks, sortby, sortreverse);
            tracks.into_iter().map(|t| t.trackhash).collect()
        }
        "album" => {
            let mut tracks = store.get_by_album(itemhash);
            tracks.sort_by(|a, b| {
                let dc = a.disc.cmp(&b.disc);
                if dc == std::cmp::Ordering::Equal {
                    a.track.cmp(&b.track)
                } else {
                    dc
                }
            });
            tracks.into_iter().map(|t| t.trackhash).collect()
        }
        "artist" => {
            let mut tracks = store.get_by_artist(itemhash);
            tracks.sort_by(|a, b| b.playcount.cmp(&a.playcount));
            tracks.into_iter().map(|t| t.trackhash).collect()
        }
        _ => Vec::new(),
    }
}

#[derive(Clone)]
struct ImgInfo {
    image: String,
    color: String,
}

fn first_4_images(
    tracks: Option<&[crate::models::Track]>,
    trackhashes: Option<&[String]>,
) -> Vec<ImgInfo> {
    let track_list: Vec<crate::models::Track> = if let Some(t) = tracks {
        t.to_vec()
    } else if let Some(hashes) = trackhashes {
        let store = TrackStore::get();
        store.get_by_hashes(hashes)
    } else {
        Vec::new()
    };

    let mut albums = Vec::new();
    for track in &track_list {
        if !albums.contains(&track.albumhash) {
            albums.push(track.albumhash.clone());
            if albums.len() == 4 {
                break;
            }
        }
    }

    let album_store = AlbumStore::get();
    let mut images: Vec<ImgInfo> = album_store
        .get_by_hashes(&albums)
        .into_iter()
        .map(|a| ImgInfo {
            image: a.image.clone(),
            color: a.color.clone(),
        })
        .collect();

    match images.len() {
        1 => {
            images = vec![
                images[0].clone(),
                images[0].clone(),
                images[0].clone(),
                images[0].clone(),
            ]
        }
        2 => {
            let mut extended = images.clone();
            extended.push(images[1].clone());
            extended.push(images[0].clone());
            images = extended;
        }
        3 => {
            images.push(images[0].clone());
        }
        _ => {}
    }

    images
}

fn build_custom_playlist(name: &str) -> (Playlist, Vec<crate::models::Track>) {
    let store = TrackStore::get();
    let mut playlist = Playlist::new(name.to_string(), None);

    let (tracks, images): (Vec<_>, Vec<_>) = if name == "recentlyplayed" {
        let hashes = crate::stores::HomepageStore::get().get_recently_played();
        let tracks = store.get_by_hashes(&hashes);
        let imgs = first_4_images(Some(&tracks), None);
        (tracks, imgs)
    } else {
        let albums = crate::stores::HomepageStore::get().get_recently_added();
        let mut tracks: Vec<crate::models::Track> = Vec::new();
        for album in albums {
            let mut t = store.get_by_album(&album);
            tracks.append(&mut t);
        }
        let imgs = first_4_images(Some(&tracks), None);
        (tracks, imgs)
    };

    let duration: i32 = tracks.iter().map(|t| t.duration).sum();
    playlist.duration = duration;
    playlist.count = tracks.len() as i32;

    (playlist, tracks)
}

fn serialize_playlist(playlist: &Playlist, images: &[ImgInfo]) -> serde_json::Value {
    let mut value = serde_json::to_value(playlist).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "images".to_string(),
            serde_json::Value::Array(
                images
                    .iter()
                    .map(|i| serde_json::json!({"image": i.image, "color": i.color}))
                    .collect(),
            ),
        );
        obj.insert(
            "pinned".to_string(),
            serde_json::json!(playlist.settings.pinned),
        );
        obj.remove("trackhashes");
    }
    value
}

fn serialize_track_for_playlist(track: &crate::models::Track) -> serde_json::Value {
    let mut value = serde_json::to_value(track).unwrap_or_else(|_| serde_json::json!({}));
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
            serde_json::Value::Bool(track.is_favorite(1)),
        );
    }

    value
}

fn sort_tracks_py(tracks: &mut [crate::models::Track], key: &str, reverse: bool) {
    if key == "default" {
        if reverse {
            tracks.reverse();
        }
        return;
    }

    // stable title sort as fallback
    tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

    match key {
        "artists" | "artist" | "albumartists" => {
            tracks.sort_by(|a, b| {
                let an = a
                    .artists
                    .get(0)
                    .map(|ar| ar.name.to_lowercase())
                    .unwrap_or_default();
                let bn = b
                    .artists
                    .get(0)
                    .map(|ar| ar.name.to_lowercase())
                    .unwrap_or_default();
                an.cmp(&bn)
            });
        }
        "album" => {
            tracks.sort_by(|a, b| a.album.to_lowercase().cmp(&b.album.to_lowercase()));
        }
        "disc" => {
            tracks.sort_by(|a, b| {
                let alb = a.album.to_lowercase().cmp(&b.album.to_lowercase());
                if alb == std::cmp::Ordering::Equal {
                    (a.disc, a.track).cmp(&(b.disc, b.track))
                } else {
                    alb
                }
            });
        }
        "title" => {
            tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        "last_mod" | "lastmod" => {
            tracks.sort_by(|a, b| a.last_mod.cmp(&b.last_mod));
        }
        _ => {}
    }

    if reverse {
        tracks.reverse();
    }
}

fn save_playlist_image(
    playlistid: i64,
    bytes: &[u8],
    content_type: &str,
    existing: Option<&str>,
) -> anyhow::Result<(String, bool)> {
    let paths = Paths::get()?;
    let dir = paths.playlist_images_dir();
    fs::create_dir_all(&dir)?;

    let is_gif = content_type.to_lowercase().contains("gif");

    let ext = if is_gif { "gif" } else { "webp" };
    let random = generate_random_string(5);
    let filename = format!("{}{}.{}", playlistid, random, ext);

    let filepath = dir.join(&filename);

    if is_gif {
        let mut file = fs::File::create(&filepath)?;
        file.write_all(bytes)?;
    }

    let thumb_path = dir.join(format!("thumb_{}", filename));

    if is_gif {
        if let Ok(img) = image::load_from_memory(bytes) {
            let thumb = resize_to_height(img, 250);
            let _ = thumb.save(&thumb_path);
        }
    } else {
        let img = image::load_from_memory(bytes)?;
        let thumb = resize_to_height(img.clone(), 250);
        thumb.save(&thumb_path)?;
        img.save_with_format(&filepath, ImageFormat::WebP)?;
    }

    if let Some(old) = existing {
        let _ = delete_playlist_images(old);
    }

    Ok((filename, is_gif))
}

fn delete_playlist_images(image_name: &str) -> anyhow::Result<()> {
    let paths = Paths::get()?;
    let dir = paths.playlist_images_dir();
    let img_path = dir.join(image_name);
    let thumb_path = dir.join(format!("thumb_{}", image_name));
    let _ = fs::remove_file(img_path);
    let _ = fs::remove_file(thumb_path);
    Ok(())
}

fn resize_to_height(img: image::DynamicImage, height: u32) -> image::DynamicImage {
    let (w, h) = img.dimensions();
    if h == 0 {
        return img;
    }
    let aspect = w as f32 / h as f32;
    let new_w = (height as f32 * aspect).round() as u32;
    img.resize_exact(new_w, height, FilterType::Lanczos3)
}

fn copy_source_image(playlist_id: i64, itemtype: &str, itemhash: &str) -> Option<String> {
    let paths = Paths::get().ok()?;
    let (source_path, content_type) = if itemtype == "artist" {
        (paths.get_artist_image_path(itemhash, "large"), "image/webp")
    } else {
        (paths.get_thumbnail_path(itemhash, "large"), "image/webp")
    };

    if !source_path.exists() {
        return None;
    }

    let bytes = fs::read(&source_path).ok()?;
    save_playlist_image(playlist_id, &bytes, content_type, None)
        .ok()
        .map(|(name, _)| name)
}

/// Configure playlist routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(send_all_playlists)
        .service(create_playlist)
        .service(add_item_to_playlist)
        .service(get_playlist)
        .service(update_playlist_info)
        .service(pin_unpin_playlist)
        .service(remove_playlist_image)
        .service(remove_playlist)
        .service(remove_tracks_from_playlist)
        .service(save_item_as_playlist);
}

/// Configure upstream prefix (/playlists)
pub fn configure_upstream(cfg: &mut web::ServiceConfig) {
    configure(cfg);
}
