//! Favorites API routes aligned with upstream Python behavior

use actix_web::{get, post, web, HttpResponse, Responder};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::db::tables::FavoriteTable;
use crate::models::{Album, Artist, Favorite, FavoriteType, Track};
use crate::stores::{AlbumStore, ArtistStore, TrackStore};
use crate::utils::dates::timestamp_to_relative;
use crate::utils::extras::get_extra_info;

const USER_ID: i64 = 0;
const API_CARD_LIMIT: i64 = 6;

#[derive(Debug, Deserialize)]
pub struct FavoritesAddBody {
    pub hash: String,
    #[serde(rename = "type")]
    pub favorite_type: FavoriteType,
}

fn default_limit() -> i64 {
    API_CARD_LIMIT
}

fn default_start() -> i64 {
    API_CARD_LIMIT
}

#[derive(Debug, Deserialize)]
pub struct GetAllOfTypeQuery {
    #[serde(default = "default_start")]
    pub start: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct GetAllFavoritesQuery {
    #[serde(default = "default_limit")]
    pub track_limit: i64,
    #[serde(default = "default_limit")]
    pub album_limit: i64,
    #[serde(default = "default_limit")]
    pub artist_limit: i64,
}

#[post("/add")]
pub async fn add_favorite(body: web::Json<FavoritesAddBody>) -> impl Responder {
    let extra = get_extra_info(&body.hash, body.favorite_type.as_str());

    if let Err(e) =
        FavoriteTable::add_with_extra(&body.hash, body.favorite_type, USER_ID, &extra).await
    {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    update_store_favorite(&body.hash, body.favorite_type, true);
    HttpResponse::Ok().json(json!({"msg": "Added to favorites"}))
}

#[post("/remove")]
pub async fn remove_favorite(body: web::Json<FavoritesAddBody>) -> impl Responder {
    if let Err(e) = FavoriteTable::remove(&body.hash, body.favorite_type, USER_ID).await {
        eprintln!("{}", e);
        return HttpResponse::InternalServerError()
            .json(json!({"msg": "Failed! An error occured"}));
    }

    update_store_favorite(&body.hash, body.favorite_type, false);
    HttpResponse::Ok().json(json!({"msg": "Removed from favorites"}))
}

#[get("/albums")]
pub async fn get_favorite_albums(query: web::Query<GetAllOfTypeQuery>) -> impl Responder {
    let (favorites, total) =
        match get_favorites_by_type(FavoriteType::Album, query.start, query.limit).await {
            Ok(res) => res,
            Err(resp) => return resp,
        };

    let hashes: Vec<String> = favorites.iter().map(|f| f.hash.clone()).collect();
    let albums = AlbumStore::get().get_by_hashes(&hashes);
    let albums: Vec<Value> = albums
        .into_iter()
        .map(|mut a| Value::Object(serialize_album_card(&mut a)))
        .collect();

    HttpResponse::Ok().json(json!({"albums": albums, "total": total}))
}

#[get("/tracks")]
pub async fn get_favorite_tracks(query: web::Query<GetAllOfTypeQuery>) -> impl Responder {
    let (favorites, total) =
        match get_favorites_by_type(FavoriteType::Track, query.start, query.limit).await {
            Ok(res) => res,
            Err(resp) => return resp,
        };

    let hashes: Vec<String> = favorites.iter().map(|f| f.hash.clone()).collect();
    let tracks = TrackStore::get().get_by_hashes(&hashes);
    let tracks: Vec<Value> = tracks
        .iter()
        .map(|t| Value::Object(serialize_track(t)))
        .collect();

    HttpResponse::Ok().json(json!({"tracks": tracks, "total": total}))
}

#[get("/artists")]
pub async fn get_favorite_artists(query: web::Query<GetAllOfTypeQuery>) -> impl Responder {
    let (favorites, total) =
        match get_favorites_by_type(FavoriteType::Artist, query.start, query.limit).await {
            Ok(res) => res,
            Err(resp) => return resp,
        };

    let hashes: Vec<String> = favorites.iter().map(|f| f.hash.clone()).collect();
    let artists = ArtistStore::get().get_by_hashes(&hashes);
    let artists: Vec<Value> = artists
        .into_iter()
        .map(|mut a| Value::Object(serialize_artist_card(&mut a)))
        .collect();

    HttpResponse::Ok().json(json!({"artists": artists, "total": total}))
}

#[get("")]
pub async fn get_all_favorites(query: web::Query<GetAllFavoritesQuery>) -> impl Responder {
    let favorites = match FavoriteTable::all(Some(USER_ID)).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"msg": "Failed! An error occured"}));
        }
    };

    let track_store = TrackStore::get();
    let album_store = AlbumStore::get();
    let artist_store = ArtistStore::get();

    let mut track_hashes = Vec::new();
    let mut album_hashes = Vec::new();
    let mut artist_hashes = Vec::new();

    for fav in &favorites {
        match fav.favorite_type {
            FavoriteType::Track => {
                if track_store.get_by_hash(&fav.hash).is_some() {
                    track_hashes.push(fav.hash.clone());
                }
            }
            FavoriteType::Album => {
                if album_store.get_by_hash(&fav.hash).is_some() {
                    album_hashes.push(fav.hash.clone());
                }
            }
            FavoriteType::Artist => {
                if artist_store.get_by_hash(&fav.hash).is_some() {
                    artist_hashes.push(fav.hash.clone());
                }
            }
        }
    }

    let track_count = track_hashes.len();
    let album_count = album_hashes.len();
    let artist_count = artist_hashes.len();

    let track_limit = query.track_limit;
    let album_limit = query.album_limit;
    let artist_limit = query.artist_limit;
    let largest = track_limit.max(album_limit).max(artist_limit) as usize;

    let tracks = track_store.get_by_hashes(
        &track_hashes
            .into_iter()
            .take(track_limit as usize)
            .collect::<Vec<_>>(),
    );
    let albums = album_store.get_by_hashes(
        &album_hashes
            .into_iter()
            .take(album_limit as usize)
            .collect::<Vec<_>>(),
    );
    let artists = artist_store.get_by_hashes(
        &artist_hashes
            .into_iter()
            .take(artist_limit as usize)
            .collect::<Vec<_>>(),
    );

    let serialized_tracks: Vec<Value> = tracks
        .iter()
        .map(|t| Value::Object(serialize_track(t)))
        .collect();
    let serialized_albums: Vec<Value> = albums
        .clone()
        .into_iter()
        .map(|mut a| Value::Object(serialize_album_card(&mut a)))
        .collect();
    let serialized_artists: Vec<Value> = artists
        .clone()
        .into_iter()
        .map(|mut a| Value::Object(serialize_artist_card(&mut a)))
        .collect();

    let mut recents: Vec<Value> = Vec::new();
    for fav in favorites {
        if recents.len() >= largest as usize {
            break;
        }

        match fav.favorite_type {
            FavoriteType::Album => {
                if let Some(album) = albums.iter().find(|a| a.albumhash == fav.hash) {
                    let mut map = serialize_album_card(&mut album.clone());
                    map.insert("help_text".to_string(), Value::String("album".to_string()));
                    map.insert(
                        "time".to_string(),
                        Value::String(timestamp_to_relative(fav.timestamp as i64)),
                    );
                    recents.push(json!({"type": "album", "item": map}));
                }
            }
            FavoriteType::Artist => {
                if let Some(artist) = artists.iter().find(|a| a.artisthash == fav.hash) {
                    let mut map = serialize_artist_card(&mut artist.clone());
                    map.insert("help_text".to_string(), Value::String("artist".to_string()));
                    map.insert(
                        "time".to_string(),
                        Value::String(timestamp_to_relative(fav.timestamp as i64)),
                    );
                    recents.push(json!({"type": "artist", "item": map}));
                }
            }
            FavoriteType::Track => {
                if let Some(track) = tracks.iter().find(|t| t.trackhash == fav.hash) {
                    let mut map = serialize_track(track);
                    map.insert("help_text".to_string(), Value::String("track".to_string()));
                    map.insert(
                        "time".to_string(),
                        Value::String(timestamp_to_relative(fav.timestamp as i64)),
                    );
                    recents.push(json!({"type": "track", "item": map}));
                }
            }
        }
    }

    recents.truncate(album_limit as usize);

    HttpResponse::Ok().json(json!({
        "recents": recents,
        "tracks": serialized_tracks,
        "albums": serialized_albums,
        "artists": serialized_artists,
        "count": {
            "tracks": track_count,
            "albums": album_count,
            "artists": artist_count,
        }
    }))
}

#[get("/check")]
pub async fn check_favorite(query: web::Query<FavoritesAddBody>) -> impl Responder {
    match FavoriteTable::exists(&query.hash, query.favorite_type, USER_ID).await {
        Ok(is_favorite) => HttpResponse::Ok().json(json!({"is_favorite": is_favorite})),
        Err(e) => {
            eprintln!("{}", e);
            HttpResponse::InternalServerError().json(json!({"msg": "Failed! An error occured"}))
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(add_favorite)
        .service(remove_favorite)
        .service(get_favorite_albums)
        .service(get_favorite_tracks)
        .service(get_favorite_artists)
        .service(get_all_favorites)
        .service(check_favorite);
}

fn update_store_favorite(hash: &str, fav_type: FavoriteType, favorite: bool) {
    match fav_type {
        FavoriteType::Track => TrackStore::get().mark_favorite(hash, favorite),
        FavoriteType::Album => AlbumStore::get().mark_favorite(hash, favorite),
        FavoriteType::Artist => ArtistStore::get().mark_favorite(hash, favorite),
    }
}

async fn get_favorites_by_type(
    fav_type: FavoriteType,
    start: i64,
    limit: i64,
) -> Result<(Vec<Favorite>, i64), HttpResponse> {
    let mut favorites: Vec<Favorite> = FavoriteTable::all(Some(USER_ID))
        .await
        .map_err(|e| {
            eprintln!("{}", e);
            HttpResponse::InternalServerError().json(json!({"msg": "Failed! An error occured"}))
        })?
        .into_iter()
        .filter(|f| f.favorite_type == fav_type)
        .collect();

    let total = if start == 0 {
        favorites.len() as i64
    } else {
        -1
    };

    let start_idx = start.max(0) as usize;
    let take_limit = if limit == -1 {
        favorites.len().saturating_sub(start_idx)
    } else {
        limit.max(0) as usize
    };
    favorites = favorites
        .into_iter()
        .skip(start_idx)
        .take(take_limit)
        .collect();

    Ok((favorites, total))
}

fn serialize_track(track: &Track) -> Map<String, Value> {
    let mut map = serde_json::to_value(track)
        .unwrap_or_else(|_| json!({}))
        .as_object()
        .cloned()
        .unwrap_or_default();

    let mut remove_keys = vec![
        "date",
        "last_mod",
        "og_title",
        "og_album",
        "copyright",
        "artisthashes",
        "created_date",
        "fav_userids",
        "playcount",
        "genrehashes",
        "id",
        "lastplayed",
        "playduration",
        "genres",
        "disc",
        "track",
        "weakhash",
        "extra",
        "pos",
        "score",
        "path",
    ];

    let dynamic_remove: Vec<String> = map
        .keys()
        .filter(|k| k.starts_with("is_") || k.starts_with('_'))
        .cloned()
        .collect();
    remove_keys.extend(dynamic_remove.iter().map(String::as_str));

    for key in remove_keys {
        map.remove(key);
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
        Value::Bool(track.is_favorite(USER_ID)),
    );

    map
}

fn serialize_album_card(album: &mut Album) -> Map<String, Value> {
    let mut map = serde_json::to_value(album)
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
        "versions",
    ] {
        map.remove(key);
    }

    if let Some(Value::Array(ref mut artists)) = map.get_mut("albumartists") {
        for artist in artists.iter_mut() {
            if let Some(obj) = artist.as_object_mut() {
                obj.remove("image");
            }
        }
    }

    map.insert("type".to_string(), Value::String("album".to_string()));
    map
}

fn serialize_artist_card(artist: &mut Artist) -> Map<String, Value> {
    let mut map = serde_json::to_value(artist)
        .unwrap_or_else(|_| json!({}))
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
