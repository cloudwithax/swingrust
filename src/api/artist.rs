//! Artist API routes

use actix_web::{get, web, HttpResponse, Responder};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::{ArtistLib, SortLib};
use crate::db::tables::SimilarArtistTable;
use crate::models::{Album, Artist, Track};
use crate::stores::{AlbumStore, ArtistStore, TrackStore};

/// Artist response
#[derive(Debug, Serialize)]
pub struct ArtistResponse {
    pub artisthash: String,
    pub name: String,
    pub image: String,
    pub color: Option<String>,
    pub is_favorite: bool,
    pub trackcount: i32,
    pub albumcount: i32,
    pub genres: Vec<String>,
}

/// Artist album response
#[derive(Debug, Serialize)]
pub struct ArtistAlbumResponse {
    pub albumhash: String,
    pub title: String,
    pub date: Option<i32>,
    pub image: String,
}

/// Artist info response
#[derive(Debug, Serialize)]
pub struct ArtistInfoResponse {
    pub artist: ArtistResponse,
    pub albums: Vec<ArtistAlbumResponse>,
    pub appearances: Vec<ArtistAlbumResponse>,
}

/// Query parameters for artist list
#[derive(Debug, Deserialize)]
pub struct ArtistListQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub sort: Option<String>,
}

/// query parameters for get_artist endpoint
#[derive(Debug, Deserialize)]
pub struct GetArtistQuery {
    /// the number of tracks to return. -1 means all tracks
    pub tracklimit: Option<i32>,
    /// the number of albums to return per category
    pub albumlimit: Option<usize>,
    /// whether to return all albums (ignores albumlimit)
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistAlbumsQuery {
    pub limit: Option<usize>,
    pub albumlimit: Option<usize>,
    pub all: Option<bool>,
}

/// query parameters for similar artists endpoint
#[derive(Debug, Deserialize)]
pub struct SimilarArtistsQuery {
    pub limit: Option<usize>,
}

/// Get all artists
#[get("")]
pub async fn get_artists(query: web::Query<ArtistListQuery>) -> impl Responder {
    let page = query.page.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);
    let sort = query.sort.as_deref().unwrap_or("name:asc");

    let mut artists = ArtistStore::get().get_all();

    // Sort artists
    let (sort_by, sort_order) = SortLib::parse_artist_sort(sort);
    SortLib::sort_artists(&mut artists, sort_by, sort_order);

    // Paginate
    let total = artists.len();
    let artists: Vec<_> = artists
        .into_iter()
        .skip(page * limit)
        .take(limit)
        .map(|a| {
            let color_val = if a.color.is_empty() {
                None
            } else {
                Some(a.color.clone())
            };
            let is_fav = a.is_favorite(1);
            let genres = a.genre_names();
            ArtistResponse {
                artisthash: a.artisthash,
                name: a.name,
                image: a.image,
                color: color_val,
                is_favorite: is_fav,
                trackcount: a.trackcount,
                albumcount: a.albumcount,
                genres,
            }
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "artists": artists,
        "total": total,
        "page": page,
        "limit": limit
    }))
}

/// Get artist by hash
#[get("/{artisthash}")]
pub async fn get_artist(
    path: web::Path<String>,
    query: web::Query<GetArtistQuery>,
) -> impl Responder {
    let artisthash = path.into_inner();
    let mut tracklimit = query.tracklimit.unwrap_or(5);
    let albumlimit = query.albumlimit.unwrap_or(7);
    let return_all_albums = query.all.unwrap_or(false);

    match ArtistLib::get_by_hash(&artisthash) {
        Some(artist) => {
            let color_val = if artist.color.is_empty() {
                None
            } else {
                Some(artist.color.clone())
            };
            let is_fav = artist.is_favorite(1);
            let mut tracks = TrackStore::get().get_by_artist(&artisthash);
            tracks.sort_by(|a, b| {
                b.date
                    .cmp(&a.date)
                    .then_with(|| a.albumhash.cmp(&b.albumhash))
                    .then_with(|| a.disc.cmp(&b.disc))
                    .then_with(|| a.track.cmp(&b.track))
            });
            let tcount = tracks.len();
            let duration: i32 = tracks.iter().map(|t| t.duration).sum();

            // override limit for small artists
            if artist.albumcount == 0 && tcount < 10 {
                tracklimit = tcount as i32;
            }

            let limit = if tracklimit == -1 {
                tcount
            } else {
                tracklimit.max(0) as usize
            };
            // clamp limit to tcount
            let limit = limit.min(tcount);

            let tracks_limited: Vec<_> = tracks
                .iter()
                .take(limit)
                .map(|t| serialize_track_with_help(t))
                .collect();

            let genres = build_genres_with_decade(&artist);
            let stats = get_track_group_stats(&tracks, false);
            let albums_grouped =
                get_artist_albums_inner(&artisthash, albumlimit, return_all_albums);

            HttpResponse::Ok().json(serde_json::json!({
                "artist": {
                    "artisthash": artist.artisthash,
                    "name": artist.name,
                    "image": artist.image,
                    "color": color_val,
                    "is_favorite": is_fav,
                    "duration": duration,
                    "trackcount": tcount as i32,
                    "albumcount": artist.albumcount,
                    "genres": genres,
                },
                "tracks": tracks_limited,
                "albums": albums_grouped,
                "stats": stats,
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({
            "error": "Artist not found"
        })),
    }
}

/// Get artist albums
#[get("/{artisthash}/albums")]
pub async fn get_artist_albums(
    path: web::Path<String>,
    query: web::Query<ArtistAlbumsQuery>,
) -> impl Responder {
    let artisthash = path.into_inner();
    let limit = query.limit.unwrap_or(7);
    let return_all = query.all.unwrap_or(false);

    let albums = get_artist_albums_inner(&artisthash, limit, return_all);
    HttpResponse::Ok().json(albums)
}

/// Configure artist routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_artists)
        .service(get_artist)
        .service(get_artist_tracks)
        .service(get_artist_albums)
        .service(get_similar_artists);
}

/// Get artist tracks (all)
#[get("/{artisthash}/tracks")]
pub async fn get_artist_tracks(path: web::Path<String>) -> impl Responder {
    let artisthash = path.into_inner();

    let mut tracks = ArtistLib::get_tracks(&artisthash);
    tracks.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.albumhash.cmp(&b.albumhash))
            .then_with(|| a.disc.cmp(&b.disc))
            .then_with(|| a.track.cmp(&b.track))
    });
    let tracks = tracks
        .into_iter()
        .map(|t| serialize_track_with_help(&t))
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(tracks)
}

/// get similar artists
#[get("/{artisthash}/similar")]
pub async fn get_similar_artists(
    path: web::Path<String>,
    query: web::Query<SimilarArtistsQuery>,
) -> impl Responder {
    let artisthash = path.into_inner();
    let limit = query.limit.unwrap_or(7);

    let similar = match SimilarArtistTable::get_similar(&artisthash).await {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!("failed to get similar artists for {}: {}", artisthash, e);
            Vec::new()
        }
    };

    if similar.is_empty() {
        return HttpResponse::Ok().json(serde_json::json!([]));
    }

    let mut artists = ArtistStore::get().get_by_hashes(&similar);

    // only sample if we have more than the limit
    if artists.len() > limit {
        artists = rand::seq::SliceRandom::choose_multiple(
            artists.as_slice(),
            &mut rand::thread_rng(),
            limit,
        )
        .cloned()
        .collect();
    }

    let serialized: Vec<_> = artists
        .into_iter()
        .take(limit)
        .map(|mut a| serialize_artist_card(&mut a))
        .collect();

    HttpResponse::Ok().json(serialized)
}

fn get_artist_albums_inner(artisthash: &str, limit: usize, return_all: bool) -> serde_json::Value {
    let entry = match ArtistStore::get().get_by_hash(artisthash) {
        Some(e) => e,
        None => return serde_json::json!({"error": "Artist not found"}),
    };

    let mut tracks = TrackStore::get().get_by_artist(artisthash);
    let mut grouped_tracks: HashMap<String, Vec<Track>> = HashMap::new();
    for track in tracks.drain(..) {
        grouped_tracks
            .entry(track.albumhash.clone())
            .or_insert_with(Vec::new)
            .push(track);
    }

    let mut albums_all: Vec<Album> = grouped_tracks
        .keys()
        .filter_map(|h| AlbumStore::get().get_by_hash(h))
        .collect();
    albums_all.sort_by(|a, b| b.date.cmp(&a.date));

    let mut res: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    res.insert("albums".to_string(), serde_json::Value::Array(vec![]));
    res.insert("appearances".to_string(), serde_json::Value::Array(vec![]));
    res.insert("compilations".to_string(), serde_json::Value::Array(vec![]));
    res.insert(
        "singles_and_eps".to_string(),
        serde_json::Value::Array(vec![]),
    );

    let serialized = |album: Album| serialize_album_card(&mut album.clone());

    let take = if return_all { albums_all.len() } else { limit };

    let mut albums_buf = Vec::new();
    let mut appearances_buf = Vec::new();
    let mut comps_buf = Vec::new();
    let mut singles_buf = Vec::new();

    for album in albums_all.into_iter() {
        let owned = album
            .albumartists
            .iter()
            .any(|a| a.artisthash == artisthash);
        let entry_tracks = grouped_tracks
            .get(&album.albumhash)
            .cloned()
            .unwrap_or_default();

        let mut album_mut = album.clone();
        album_mut.set_type(&entry_tracks);

        match album_mut.album_type.as_str() {
            "single" | "ep" => singles_buf.push(album_mut),
            "compilation" => comps_buf.push(album_mut),
            _ => {
                if !owned {
                    appearances_buf.push(album_mut);
                } else {
                    albums_buf.push(album_mut);
                }
            }
        }
    }

    let to_array = |list: Vec<Album>| {
        let mut l = list;
        l.truncate(take);
        l.into_iter()
            .map(|a| serde_json::Value::Object(serialized(a)))
            .collect()
    };

    res.insert(
        "albums".to_string(),
        serde_json::Value::Array(to_array(albums_buf)),
    );
    res.insert(
        "appearances".to_string(),
        serde_json::Value::Array(to_array(appearances_buf)),
    );
    res.insert(
        "compilations".to_string(),
        serde_json::Value::Array(to_array(comps_buf)),
    );
    res.insert(
        "singles_and_eps".to_string(),
        serde_json::Value::Array(to_array(singles_buf)),
    );
    res.insert("artistname".to_string(), serde_json::json!(entry.name));

    serde_json::Value::Object(res)
}

fn serialize_artist_card(artist: &mut Artist) -> serde_json::Value {
    let mut map = serde_json::to_value(artist)
        .unwrap_or_else(|_| serde_json::json!({}))
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

    map.insert(
        "type".to_string(),
        serde_json::Value::String("artist".to_string()),
    );
    serde_json::Value::Object(map)
}

fn serialize_album_card(album: &mut Album) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::to_value(album)
        .unwrap_or_else(|_| serde_json::json!({}))
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
        "pos",
        "score",
    ] {
        map.remove(key);
    }

    for artist in ["artists", "albumartists"] {
        if let Some(serde_json::Value::Array(items)) = map.get_mut(artist) {
            for item in items {
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("image");
                }
            }
        }
    }

    map.insert(
        "type".to_string(),
        serde_json::Value::String("album".to_string()),
    );
    map
}

fn serialize_track_with_help(track: &Track) -> serde_json::Value {
    let mut map = serde_json::to_value(track)
        .unwrap_or_else(|_| serde_json::json!({}))
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
    let help = if track.playcount == 0 {
        "unplayed".to_string()
    } else {
        format!(
            "{} play{}",
            track.playcount,
            if track.playcount == 1 { "" } else { "s" }
        )
    };
    map.insert("help_text".to_string(), serde_json::Value::String(help));

    serde_json::Value::Object(map)
}

fn build_genres_with_decade(artist: &Artist) -> Vec<serde_json::Value> {
    let mut genres: Vec<serde_json::Value> = artist
        .genres
        .iter()
        .map(|g| serde_json::json!({"name": g.name, "genrehash": g.genrehash}))
        .collect();

    if artist.date > 0 {
        if let Some(year) = DateTime::<Utc>::from_timestamp(artist.date, 0).map(|dt| dt.year()) {
            let decade = (year / 10) * 10;
            let label = format!("{}s", decade % 100);
            genres.insert(0, serde_json::json!({"name": label, "genrehash": label}));
        }
    }

    genres
}

fn get_track_group_stats(tracks: &[Track], is_album: bool) -> Vec<serde_json::Value> {
    if tracks.is_empty() {
        return Vec::new();
    }

    let played_tracks: Vec<&Track> = tracks.iter().filter(|t| t.playcount > 0).collect();
    let unplayed_count = tracks.len().saturating_sub(played_tracks.len());

    let play_duration: i32 = played_tracks.iter().map(|t| t.playduration).sum();

    let played_stat = serde_json::json!({
        "cssclass": "played",
        "text": "never played",
        "value": format!("{}/{} tracks", unplayed_count, tracks.len())
    });

    let play_duration_stat = serde_json::json!({
        "cssclass": "play_duration",
        "text": "listened all time",
        "value": seconds_to_time_string(play_duration as i64),
    });

    let top_track = played_tracks
        .iter()
        .max_by_key(|t| t.playduration)
        .map(|t| serde_json::json!({
            "cssclass": "toptrack",
            "text": format!("top track ({} listened)", seconds_to_time_string(t.playduration as i64)),
            "value": t.title,
            "image": t.image,
        }))
        .unwrap_or_else(|| serde_json::json!({
            "cssclass": "toptrack",
            "text": "top track",
            "value": "-",
        }));

    let mut stats = vec![play_duration_stat, played_stat, top_track];

    if !is_album {
        // group by album
        let mut album_map: std::collections::HashMap<String, (String, i32, String)> =
            std::collections::HashMap::new();
        for t in tracks {
            let entry = album_map
                .entry(t.albumhash.clone())
                .or_insert_with(|| (t.album.clone(), 0, t.image.clone()));
            entry.1 += t.playduration;
            if entry.2.is_empty() {
                entry.2 = t.image.clone();
            }
        }
        let mut albums: Vec<_> = album_map.into_iter().collect();
        albums.sort_by_key(|(_, val)| -val.1);
        if let Some((_, (title, playdur, image))) = albums.first() {
            stats.push(serde_json::json!({
                "cssclass": "topalbum",
                "text": format!("top album ({} listened)", seconds_to_time_string(*playdur as i64)),
                "value": title,
                "image": image,
            }));
        }
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
