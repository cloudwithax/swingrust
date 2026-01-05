//! Album API routes (upstream-compatible)

use actix_web::{get, post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::core::{AlbumLib, SortLib};
use crate::db::tables::SimilarArtistTable;
use crate::models::{Album, Track};
use crate::stores::{AlbumStore, TrackStore};
use crate::utils::hashing::create_hash;

const USER_ID: i64 = 0;

/// Album response
#[derive(Debug, Serialize)]
pub struct AlbumResponse {
    pub albumhash: String,
    pub title: String,
    pub albumartist: String,
    pub date: Option<i32>,
    pub duration: i32,
    pub count: i32,
    pub image: String,
    pub color: Option<String>,
    pub is_favorite: bool,
    pub genres: Vec<String>,
}

/// Track in album response
#[derive(Debug, Serialize)]
pub struct AlbumTrackResponse {
    pub trackhash: String,
    pub title: String,
    pub artist: String,
    pub duration: i32,
    pub track: Option<i32>,
    pub disc: Option<i32>,
}

/// Album info response (legacy GET)
#[derive(Debug, Serialize)]
pub struct AlbumInfoResponse {
    pub album: AlbumResponse,
    pub tracks: Vec<AlbumTrackResponse>,
    pub versions: Vec<serde_json::Value>,
}

/// Stat item response (parity with upstream StatItem)
#[derive(Debug, Serialize)]
pub struct StatItem {
    pub cssclass: String,
    pub text: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Query parameters for album list
#[derive(Debug, Deserialize)]
pub struct AlbumListQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub sort: Option<String>,
}

fn default_album_limit() -> i64 {
    6
}

#[derive(Debug, Deserialize)]
pub struct AlbumInfoBody {
    pub albumhash: String,
    #[serde(default = "default_album_limit", alias = "albumlimit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct MoreFromArtistsBody {
    pub albumartists: Vec<String>,
    pub base_title: String,
    #[serde(default = "default_album_limit", alias = "albumlimit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct AlbumVersionsBody {
    pub og_album_title: String,
    pub albumhash: String,
}

#[derive(Debug, Deserialize)]
pub struct SimilarAlbumsQuery {
    pub artisthash: String,
    #[serde(default = "default_album_limit", alias = "albumlimit")]
    pub limit: i64,
}

/// Get all albums
#[get("")]
pub async fn get_albums(query: web::Query<AlbumListQuery>) -> impl Responder {
    let page = query.page.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    let sort = query.sort.as_deref().unwrap_or("title:asc");

    let mut albums = AlbumStore::get().get_all();

    // Sort albums
    let (sort_by, sort_order) = SortLib::parse_album_sort(sort);
    SortLib::sort_albums(&mut albums, sort_by, sort_order);

    // Paginate
    let total = albums.len();
    let albums: Vec<_> = albums
        .into_iter()
        .skip(page * limit)
        .take(limit)
        .map(|a| AlbumResponse {
            albumhash: a.albumhash.clone(),
            title: a.title.clone(),
            albumartist: a.albumartist(),
            date: if a.date > 0 {
                Some(a.date as i32)
            } else {
                None
            },
            duration: a.duration,
            count: a.count(),
            image: a.image.clone(),
            color: if a.color.is_empty() {
                None
            } else {
                Some(a.color.clone())
            },
            is_favorite: a.is_favorite(USER_ID),
            genres: a.genre_names(),
        })
        .collect();

    HttpResponse::Ok().json(json!({
        "albums": albums,
        "total": total,
        "page": page,
        "limit": limit
    }))
}

/// Upstream-compatible album info (POST /album)
#[post("")]
pub async fn get_album_info(body: web::Json<AlbumInfoBody>) -> impl Responder {
    let albumhash = &body.albumhash;
    let limit = body.limit.max(0) as usize;

    let Some(mut album) = AlbumStore::get().get_by_hash(albumhash) else {
        return HttpResponse::NotFound().json(json!({"error": "Album not found"}));
    };

    let tracks = TrackStore::get().get_by_album(albumhash);

    album.trackcount = tracks.len() as i32;
    album.duration = tracks.iter().map(|t| t.duration).sum();
    album.set_type(&tracks);

    // Python: sum({int(t.extra.get("track_total", 1) or 1) for t in tracks})
    // Creates a set of unique track_total values, then sums them
    let track_total: i32 = tracks
        .iter()
        .map(|t| {
            t.extra
                .get("track_total")
                .and_then(|v| v.as_i64())
                .unwrap_or(1)
                .max(1) as i32
        })
        .collect::<std::collections::HashSet<_>>()
        .iter()
        .sum();
    let avg_bitrate: i32 = if tracks.is_empty() {
        0
    } else {
        tracks.iter().map(|t| t.bitrate).sum::<i32>() / (tracks.len() as i32)
    };

    let stats = build_track_group_stats(&tracks, true);

    let mut info = serde_json::to_value(&album).unwrap_or_else(|_| json!({}));
    if let Some(map) = info.as_object_mut() {
        map.insert("is_favorite".to_string(), json!(album.is_favorite(USER_ID)));
        map.remove("help_text");
    }

    let serialized_tracks: Vec<_> = tracks
        .iter()
        .map(|t| serialize_track_for_album(t, false))
        .collect();

    let more_from = get_more_from_artist_inner(MoreFromArtistsBody {
        albumartists: album
            .albumartists
            .iter()
            .map(|a| a.artisthash.clone())
            .collect(),
        base_title: album.base_title.clone(),
        limit: limit as i64,
    });

    let other_versions = get_album_versions_inner(AlbumVersionsBody {
        og_album_title: album.og_title.clone(),
        albumhash: albumhash.clone(),
    });

    let copyright = tracks
        .first()
        .and_then(|t| t.copyright.clone())
        .unwrap_or_default();

    HttpResponse::Ok().json(json!({
        "stats": stats,
        "info": info,
        "extra": {
            "track_total": track_total,
            "avg_bitrate": avg_bitrate,
        },
        "copyright": copyright,
        "tracks": serialized_tracks,
        "more_from": more_from,
        "other_versions": other_versions,
    }))
}

/// Get album by hash (legacy GET)
#[get("/{albumhash}")]
pub async fn get_album(path: web::Path<String>) -> impl Responder {
    let albumhash = path.into_inner();

    match AlbumStore::get().get_by_hash(&albumhash) {
        Some(album) => {
            let tracks = AlbumLib::get_tracks(&albumhash);
            let versions = get_album_versions_inner(AlbumVersionsBody {
                og_album_title: album.og_title.clone(),
                albumhash: albumhash.clone(),
            });

            let response = AlbumInfoResponse {
                album: AlbumResponse {
                    albumhash: album.albumhash.clone(),
                    title: album.title.clone(),
                    albumartist: album.albumartist(),
                    date: if album.date > 0 {
                        Some(album.date as i32)
                    } else {
                        None
                    },
                    duration: album.duration,
                    count: album.count(),
                    image: album.image.clone(),
                    color: if album.color.is_empty() {
                        None
                    } else {
                        Some(album.color.clone())
                    },
                    is_favorite: album.is_favorite(USER_ID),
                    genres: album.genre_names(),
                },
                tracks: tracks
                    .into_iter()
                    .map(|t| AlbumTrackResponse {
                        trackhash: t.trackhash.clone(),
                        title: t.title.clone(),
                        artist: t.artist(),
                        duration: t.duration,
                        track: if t.track > 0 { Some(t.track) } else { None },
                        disc: if t.disc > 0 { Some(t.disc) } else { None },
                    })
                    .collect(),
                versions,
            };

            HttpResponse::Ok().json(response)
        }
        None => HttpResponse::NotFound().json(json!({
            "error": "Album not found"
        })),
    }
}

/// Get album tracks
#[get("/{albumhash}/tracks")]
pub async fn get_album_tracks(path: web::Path<String>) -> impl Responder {
    let albumhash = path.into_inner();

    let tracks = AlbumLib::get_tracks(&albumhash);

    let response: Vec<_> = tracks
        .into_iter()
        .map(|t| AlbumTrackResponse {
            trackhash: t.trackhash.clone(),
            title: t.title.clone(),
            artist: t.artist(),
            duration: t.duration,
            track: if t.track > 0 { Some(t.track) } else { None },
            disc: if t.disc > 0 { Some(t.disc) } else { None },
        })
        .collect();

    HttpResponse::Ok().json(response)
}

/// Get more albums from the given artists (upstream parity)
#[post("/from-artist")]
pub async fn get_more_from_artist(body: web::Json<MoreFromArtistsBody>) -> impl Responder {
    HttpResponse::Ok().json(json!(get_more_from_artist_inner(body.into_inner())))
}

/// Get other versions of the given album (upstream parity)
#[post("/other-versions")]
pub async fn get_album_versions(body: web::Json<AlbumVersionsBody>) -> impl Responder {
    HttpResponse::Ok().json(json!(get_album_versions_inner(body.into_inner())))
}

/// Get similar albums based on similar artists
#[get("/similar")]
pub async fn get_similar_albums(query: web::Query<SimilarAlbumsQuery>) -> impl Responder {
    let limit = query.limit.max(0) as usize;
    let artists = match SimilarArtistTable::get_similar(&query.artisthash).await {
        Ok(list) => list,
        Err(_) => Vec::new(),
    };

    if artists.is_empty() {
        return HttpResponse::Ok().json(json!([]));
    }

    let mut albums: Vec<Album> = artists
        .iter()
        .flat_map(|hash| AlbumStore::get().get_by_artist(hash))
        .collect();

    if albums.is_empty() {
        return HttpResponse::Ok().json(json!([]));
    }

    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    albums.shuffle(&mut rng);
    albums.truncate(limit);

    let serialized: Vec<_> = albums
        .into_iter()
        .map(|mut a| serialize_album_card(&mut a))
        .collect();

    HttpResponse::Ok().json(json!(serialized))
}

fn serialize_album_card(album: &Album) -> serde_json::Value {
    // Python serialize_for_card removes: duration, count, artisthashes, albumartists_hashes,
    // created_date, og_title, base_title, genres, playcount, trackcount, type, playduration,
    // genrehashes, fav_userids, extra, id, lastplayed, weakhash
    let mut value = serde_json::to_value(album).unwrap_or_else(|_| json!({}));
    if let Some(map) = value.as_object_mut() {
        let to_remove = vec![
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
        ];

        for key in to_remove {
            map.remove(key);
        }

        // Remove keys starting with "is_"
        let keys_to_remove: Vec<String> = map
            .keys()
            .filter(|k| k.starts_with("is_"))
            .cloned()
            .collect();
        for key in keys_to_remove {
            map.remove(&key);
        }

        // Remove artist images
        if let Some(arr) = map.get_mut("albumartists").and_then(|v| v.as_array_mut()) {
            for artist in arr {
                if let Some(obj) = artist.as_object_mut() {
                    obj.remove("image");
                }
            }
        }

        map.insert("type".to_string(), json!("album"));
    }
    value
}

fn serialize_track_for_album(track: &Track, remove_disc: bool) -> serde_json::Value {
    let mut value = serde_json::to_value(track).unwrap_or_else(|_| json!({}));
    if let Some(map) = value.as_object_mut() {
        let mut to_remove: HashSet<String> = [
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

        if let Some(arr) = map.get_mut("artists").and_then(|v| v.as_array_mut()) {
            for artist in arr {
                if let Some(obj) = artist.as_object_mut() {
                    obj.remove("image");
                }
            }
        }

        if let Some(arr) = map.get_mut("albumartists").and_then(|v| v.as_array_mut()) {
            for artist in arr {
                if let Some(obj) = artist.as_object_mut() {
                    obj.remove("image");
                }
            }
        }

        // Add computed fields that must be present in the output
        map.insert(
            "is_favorite".to_string(),
            serde_json::Value::Bool(track.is_favorite(USER_ID)),
        );
    }

    value
}

fn build_track_group_stats(tracks: &[Track], is_album: bool) -> Vec<StatItem> {
    if tracks.is_empty() {
        return Vec::new();
    }

    let played_tracks: Vec<&Track> = tracks.iter().filter(|t| t.playcount > 0).collect();
    let unplayed_count = tracks.len().saturating_sub(played_tracks.len());

    let played_stat = StatItem {
        cssclass: "played".to_string(),
        text: "never played".to_string(),
        value: format!("{}/{} tracks", unplayed_count, tracks.len()),
        image: None,
    };

    let play_duration: i64 = played_tracks.iter().map(|t| t.playduration as i64).sum();
    let play_duration_stat = StatItem {
        cssclass: "play_duration".to_string(),
        text: "listened all time".to_string(),
        value: seconds_to_time_string(play_duration),
        image: None,
    };

    let top_track = played_tracks.iter().max_by_key(|t| t.playduration).copied();
    let top_track_stat = match top_track {
        Some(t) => StatItem {
            cssclass: "toptrack".to_string(),
            text: format!(
                "top track ({} listened)",
                seconds_to_time_string(t.playduration as i64)
            ),
            value: t.title.clone(),
            image: if t.image.is_empty() {
                None
            } else {
                Some(t.image.clone())
            },
        },
        None => StatItem {
            cssclass: "toptrack".to_string(),
            text: "top track".to_string(),
            value: "—".to_string(),
            image: None,
        },
    };

    let mut stats = vec![play_duration_stat, played_stat, top_track_stat];

    if !is_album {
        let mut albums_map: HashMap<String, (String, i32, String)> = HashMap::new();
        for track in tracks {
            let entry = albums_map
                .entry(track.albumhash.clone())
                .or_insert_with(|| (track.album.clone(), 0, track.image.clone()));
            entry.1 += track.playduration;
            if entry.2.is_empty() {
                entry.2 = track.image.clone();
            }
        }

        let mut albums: Vec<_> = albums_map.into_values().collect();
        albums.sort_by(|a, b| b.1.cmp(&a.1));

        let top_album_stat = albums
            .first()
            .filter(|(_, playduration, _)| *playduration > 0)
            .map(|(title, playduration, image)| StatItem {
                cssclass: "topalbum".to_string(),
                text: format!(
                    "top album ({} listened)",
                    seconds_to_time_string(*playduration as i64)
                ),
                value: title.clone(),
                image: if image.is_empty() {
                    None
                } else {
                    Some(image.clone())
                },
            })
            .unwrap_or_else(|| StatItem {
                cssclass: "topalbum".to_string(),
                text: "top album".to_string(),
                value: "ƒ?\"".to_string(),
                image: None,
            });

        stats.push(top_album_stat);
    } else {
        let track_total: i32 = tracks
            .iter()
            .filter_map(|t| t.extra.get("track_total").and_then(|v| v.as_i64()))
            .map(|v| v as i32)
            .max()
            .unwrap_or(0);

        let percentage = if track_total > 0 {
            (tracks.len() as f64 / track_total as f64) * 100.0
        } else {
            101.0
        };

        let completedness = if percentage <= 100.0 {
            Some(percentage as i32)
        } else {
            None
        };

        let track_total_text = if track_total == 0 {
            "?".to_string()
        } else {
            track_total.to_string()
        };

        stats.push(StatItem {
            cssclass: "completeness".to_string(),
            text: format!("{}/{} tracks available", tracks.len(), track_total_text),
            value: if track_total == 0 {
                "?".to_string()
            } else if let Some(val) = completedness {
                format!("{}% complete", val)
            } else {
                "?".to_string()
            },
            image: None,
        });
    }

    stats
}

fn seconds_to_time_string(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let remaining_seconds = seconds % 60;

    if hours > 0 {
        if minutes > 0 {
            return format!(
                "{} hr{}, {} min{}",
                hours,
                if hours > 1 { "s" } else { "" },
                minutes,
                if minutes > 1 { "s" } else { "" }
            );
        }

        return format!("{} hr{}", hours, if hours > 1 { "s" } else { "" });
    }

    if minutes > 0 {
        return format!("{} min{}", minutes, if minutes > 1 { "s" } else { "" });
    }

    format!("{} sec", remaining_seconds)
}

fn get_more_from_artist_inner(
    body: MoreFromArtistsBody,
) -> HashMap<String, Vec<serde_json::Value>> {
    let mut result: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    let base_hash = create_hash(&[body.base_title.as_str()], false);

    for artisthash in body.albumartists {
        let mut filtered: Vec<_> = AlbumLib::get_by_artist(&artisthash)
            .into_iter()
            .filter(|album| artisthash_is_in_album(&artisthash, album))
            .filter(|album| create_hash(&[album.base_title.as_str()], false) != base_hash)
            .filter(|album| !seen.contains(&album.albumhash))
            .collect();

        filtered.truncate(body.limit as usize);
        for album in &filtered {
            seen.insert(album.albumhash.clone());
        }

        let cards = filtered
            .into_iter()
            .map(|album| serialize_album_card(&album))
            .collect();

        result.insert(artisthash, cards);
    }

    result
}

fn get_album_versions_inner(body: AlbumVersionsBody) -> Vec<serde_json::Value> {
    let Some(album) = AlbumLib::get_by_hash(&body.albumhash) else {
        return Vec::new();
    };

    if album.artisthashes.is_empty() {
        return Vec::new();
    }

    let primary_artist = album.artisthashes[0].clone();
    let base_title = album.base_title.clone();

    let mut versions = Vec::new();
    for candidate in AlbumLib::get_by_artist(&primary_artist) {
        if candidate.albumhash == album.albumhash {
            continue;
        }

        let candidate_base = candidate.base_title.clone();

        // Python uses exact string comparison, not hash comparison
        if candidate_base != base_title {
            continue;
        }

        if !artisthash_is_in_album(&primary_artist, &candidate) {
            continue;
        }

        if candidate.og_title == body.og_album_title {
            continue;
        }

        versions.push(serialize_album_card(&candidate));
    }

    versions
}

fn artisthash_is_in_album(hash: &str, album: &Album) -> bool {
    album.artisthashes.iter().any(|h| h == hash)
}

/// Configure album routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_albums)
        .service(get_album)
        .service(get_album_tracks)
        .service(get_album_info)
        .service(get_more_from_artist)
        .service(get_album_versions)
        .service(get_similar_albums);
}
