//! logger and stats api routes mirroring upstream flask behavior

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

use crate::config::UserConfig;
use crate::core::homepage::HomepageStore;
use crate::db::tables::{FavoriteTable, ScrobbleTable};
use crate::models::{Album, Artist, Track};
use crate::plugins::LastFmPlugin;
use crate::stores::{AlbumStore, ArtistStore, TrackStore};
use crate::utils::auth::verify_jwt;
use crate::utils::dates::{start_of_month, start_of_week, start_of_year};
use crate::utils::extras::get_extra_info;

const DEFAULT_USER_ID: i64 = 0;

/// log track request payload
#[derive(Debug, Deserialize)]
pub struct LogTrackRequest {
    pub trackhash: String,
    pub timestamp: i64,
    pub duration: i32,
    #[serde(default)]
    pub source: String,
}

/// chart query params
#[derive(Debug, Deserialize)]
pub struct ChartQuery {
    #[serde(default = "default_duration")]
    pub duration: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_order_by")]
    pub order_by: String,
}

fn default_duration() -> String {
    "year".to_string()
}

fn default_limit() -> usize {
    10
}

fn default_order_by() -> String {
    "playduration".to_string()
}

/// stat item aligned with upstream
#[derive(Debug, Serialize)]
pub struct StatItem {
    pub cssclass: String,
    pub text: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// log a track play
#[post("/track/log")]
pub async fn log_track(req: HttpRequest, body: web::Json<LogTrackRequest>) -> impl Responder {
    if body.timestamp == 0 || body.duration < 5 {
        return HttpResponse::BadRequest().json(json!({"msg": "Invalid entry."}));
    }

    let track = match TrackStore::get().get_by_hash(&body.trackhash) {
        Some(t) => t,
        None => {
            return HttpResponse::NotFound().json(json!({"msg": "Track not found."}));
        }
    };

    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let extra = get_extra_info(&body.trackhash, "track");
    if let Err(e) = ScrobbleTable::add_with_extra(
        &body.trackhash,
        body.timestamp,
        body.duration,
        &body.source,
        user_id,
        &extra,
    )
    .await
    {
        return HttpResponse::InternalServerError()
            .json(json!({"msg": format!("Failed to log track: {}", e)}));
    }

    HomepageStore::get().update_recently_played(user_id).await;

    let albumhash = track.albumhash.clone();
    TrackStore::get().increment_play_stats(&body.trackhash, body.duration, body.timestamp);
    AlbumStore::get().increment_play_stats(&albumhash, body.duration, body.timestamp);
    for artisthash in &track.artisthashes {
        ArtistStore::get().increment_play_stats(artisthash, body.duration, body.timestamp);
    }

    if LastFmPlugin::should_scrobble(track.duration, body.duration) {
        if let Some(session_key) = lastfm_session_for_user(user_id) {
            let plugin = LastFmPlugin::new();
            let scrobble_track = track.clone();
            let play_timestamp = body.timestamp;

            tokio::spawn(async move {
                if let Err(err) = plugin
                    .scrobble(&scrobble_track, play_timestamp, &session_key)
                    .await
                {
                    eprintln!("lastfm scrobble error: {}", err);
                }
            });
        }
    }

    HttpResponse::Created().json(json!({"msg": "recorded"}))
}

/// top tracks
#[get("/top-tracks")]
pub async fn get_top_tracks(req: HttpRequest, query: web::Query<ChartQuery>) -> impl Responder {
    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let (start_time, end_time) = get_date_range(&query.duration);
    let previous_start_time = start_time - get_duration_in_seconds(&query.duration);

    let (current_tracks, current_scrobbles, duration) =
        get_tracks_in_period(user_id, start_time, end_time).await;
    let (previous_tracks, previous_scrobbles, _) =
        get_tracks_in_period(user_id, previous_start_time, start_time).await;

    let scrobble_trend = calculate_scrobble_trend(current_scrobbles, previous_scrobbles);

    let mut sorted_tracks = sort_tracks(current_tracks.clone(), &query.order_by);
    let top_tracks: Vec<Value> = sorted_tracks
        .drain(..)
        .take(query.limit)
        .filter_map(|track| {
            let trend = calculate_track_trend(&track, &current_tracks, &previous_tracks);
            let help_text = get_help_text(track.playcount, track.playduration, &query.order_by);
            let mut map = serialize_track_for_stats(&track);
            map.insert("trend".to_string(), trend);
            map.insert("help_text".to_string(), Value::String(help_text));
            Some(Value::Object(map))
        })
        .collect();

    HttpResponse::Ok().json(json!({
        "tracks": top_tracks,
        "scrobbles": {
            "text": format!("{} total play{} ({})", current_scrobbles, if current_scrobbles == 1 { "" } else { "s" }, seconds_to_time_string(duration as i64)),
            "trend": scrobble_trend,
            "dates": format_date_range(start_time, end_time),
        }
    }))
}

/// top artists
#[get("/top-artists")]
pub async fn get_top_artists(req: HttpRequest, query: web::Query<ChartQuery>) -> impl Responder {
    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let (start_time, end_time) = get_date_range(&query.duration);
    let previous_start_time = start_time - get_duration_in_seconds(&query.duration);

    let current_artists = get_artists_in_period(user_id, start_time, end_time).await;
    let previous_artists = get_artists_in_period(user_id, previous_start_time, start_time).await;

    let new_artists = calculate_new_artists(&current_artists, start_time, user_id).await;
    let scrobble_trend =
        calculate_scrobble_trend(current_artists.len() as i32, previous_artists.len() as i32);

    let mut sorted_artists = sort_artists(current_artists.clone(), &query.order_by);
    let top_artists: Vec<Value> = sorted_artists
        .drain(..)
        .take(query.limit)
        .filter_map(|artist| {
            let trend = calculate_artist_trend(&artist, &current_artists, &previous_artists);
            let db_artist = ArtistStore::get().get_by_hash(&artist.artisthash)?;

            let mut map = serialize_artist_card(&mut db_artist.clone());
            map.insert("trend".to_string(), trend);
            map.insert(
                "help_text".to_string(),
                Value::String(get_help_text(
                    artist.playcount,
                    artist.playduration,
                    &query.order_by,
                )),
            );
            map.insert("extra".to_string(), json!({"playcount": artist.playcount}));

            Some(Value::Object(map))
        })
        .collect();

    HttpResponse::Ok().json(json!({
        "artists": top_artists,
        "scrobbles": {
            "text": format!("{} {} {}", new_artists, if query.duration != "alltime" { "new" } else { "" }, if new_artists == 1 { "artist" } else { "artists" }).trim().to_string(),
            "trend": scrobble_trend,
            "dates": format_date_range(start_time, end_time),
        }
    }))
}

/// top albums
#[get("/top-albums")]
pub async fn get_top_albums(req: HttpRequest, query: web::Query<ChartQuery>) -> impl Responder {
    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let (start_time, end_time) = get_date_range(&query.duration);
    let previous_start_time = start_time - get_duration_in_seconds(&query.duration);

    let current_albums = get_albums_in_period(user_id, start_time, end_time).await;
    let previous_albums = get_albums_in_period(user_id, previous_start_time, start_time).await;

    let new_albums = calculate_new_albums(&current_albums, &previous_albums);
    let scrobble_trend =
        calculate_scrobble_trend(current_albums.len() as i32, previous_albums.len() as i32);

    let mut sorted_albums = sort_albums(current_albums.clone(), &query.order_by);
    let top_albums: Vec<Value> = sorted_albums
        .drain(..)
        .take(query.limit)
        .map(|album| {
            let trend = calculate_album_trend(&album, &current_albums, &previous_albums);
            let mut map = serialize_album_card(&mut album.clone());
            map.insert("trend".to_string(), trend);
            map.insert(
                "help_text".to_string(),
                Value::String(get_help_text(
                    album.playcount,
                    album.playduration,
                    &query.order_by,
                )),
            );
            Value::Object(map)
        })
        .collect();

    HttpResponse::Ok().json(json!({
        "albums": top_albums,
        "scrobbles": {
            "text": format!("{} new album{} played", new_albums, if new_albums == 1 { "" } else { "s" }),
            "trend": scrobble_trend,
            "dates": format_date_range(start_time, end_time),
        }
    }))
}

/// stats dashboard
#[get("/stats")]
pub async fn get_stats(req: HttpRequest) -> impl Responder {
    let user_id = match resolve_user_id(&req).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let period = "week";
    let (start_time, end_time) = get_date_range(period);

    let said_period = match period {
        "week" => "this week",
        "month" => "this month",
        "year" => "this year",
        "alltime" => "all time",
        _ => "this week",
    };

    let count = TrackStore::get().get_all().len() as i64;
    let total_tracks = StatItem {
        cssclass: "trackcount".to_string(),
        text: "in your library".to_string(),
        value: format!("{} {}", count, if count == 1 { "track" } else { "tracks" }),
        image: None,
    };

    let (tracks, playcount_total, playduration_total) =
        get_tracks_in_period(user_id, start_time, end_time).await;

    let playcount = StatItem {
        cssclass: "streams".to_string(),
        text: said_period.to_string(),
        value: format!(
            "{} track {}",
            playcount_total,
            if playcount_total == 1 {
                "play"
            } else {
                "plays"
            }
        ),
        image: None,
    };

    let playduration = StatItem {
        cssclass: "playtime".to_string(),
        text: said_period.to_string(),
        value: format!(
            "{} listened",
            seconds_to_time_string(playduration_total as i64)
        ),
        image: None,
    };

    let sorted_tracks = sort_tracks(tracks.clone(), "playduration");
    let top_track = if let Some(track) = sorted_tracks.first() {
        StatItem {
            cssclass: "toptrack".to_string(),
            text: format!("Top track {}", said_period),
            value: format!("{} - {}", track.title, track.artist()),
            image: if track.image.is_empty() {
                None
            } else {
                Some(track.image.clone())
            },
        }
    } else {
        StatItem {
            cssclass: "toptrack".to_string(),
            text: format!("Top track {}", said_period),
            value: "-".to_string(),
            image: None,
        }
    };

    let fav_count = FavoriteTable::count_in_range(user_id, start_time, end_time)
        .await
        .unwrap_or(0);
    let favorites = StatItem {
        cssclass: "favorites".to_string(),
        text: said_period.to_string(),
        value: format!(
            "{} {}favorite{}",
            fav_count,
            if period != "alltime" { "new " } else { "" },
            if fav_count == 1 { "" } else { "s" }
        ),
        image: None,
    };

    HttpResponse::Ok().json(json!({
        "stats": [
            top_track,
            playcount,
            playduration,
            favorites,
            total_tracks,
        ],
        "dates": format_date_range(start_time, end_time),
    }))
}

/// configure logger routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(log_track)
        .service(get_top_tracks)
        .service(get_top_artists)
        .service(get_top_albums)
        .service(get_stats);
}

// helpers

async fn resolve_user_id(req: &HttpRequest) -> Result<i64, HttpResponse> {
    match optional_user_id(req).await? {
        Some(id) => Ok(id),
        None => Ok(DEFAULT_USER_ID),
    }
}

async fn optional_user_id(req: &HttpRequest) -> Result<Option<i64>, HttpResponse> {
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

    let user_id = claims.sub.id;
    Ok(Some(user_id))
}

fn lastfm_session_for_user(user_id: i64) -> Option<String> {
    let config = UserConfig::load().ok()?;
    config.get_lastfm_session_key(&user_id.to_string()).cloned()
}

fn get_help_text(playcount: i32, playduration: i32, order_by: &str) -> String {
    if order_by == "playcount" {
        if playcount == 0 {
            "unplayed".to_string()
        } else {
            format!(
                "{} play{}",
                playcount,
                if playcount == 1 { "" } else { "s" }
            )
        }
    } else {
        seconds_to_time_string(playduration as i64)
    }
}

fn sort_tracks(tracks: Vec<Track>, order_by: &str) -> Vec<Track> {
    let mut sorted = tracks;
    match order_by {
        "playcount" => sorted.sort_by(|a, b| b.playcount.cmp(&a.playcount)),
        _ => sorted.sort_by(|a, b| b.playduration.cmp(&a.playduration)),
    }
    sorted
}

fn sort_artists(artists: Vec<ArtistPeriod>, order_by: &str) -> Vec<ArtistPeriod> {
    let mut sorted = artists;
    match order_by {
        "playcount" => sorted.sort_by(|a, b| b.playcount.cmp(&a.playcount)),
        _ => sorted.sort_by(|a, b| b.playduration.cmp(&a.playduration)),
    }
    sorted
}

fn sort_albums(albums: Vec<Album>, order_by: &str) -> Vec<Album> {
    let mut sorted = albums;
    match order_by {
        "playcount" => sorted.sort_by(|a, b| b.playcount.cmp(&a.playcount)),
        _ => sorted.sort_by(|a, b| b.playduration.cmp(&a.playduration)),
    }
    sorted
}

fn calculate_trend<T, F>(item: &T, current_items: &[T], previous_items: &[T], key_func: F) -> Value
where
    T: Clone,
    F: Fn(&T) -> String,
{
    let current_rank = current_items
        .iter()
        .position(|t| key_func(t) == key_func(item))
        .map(|i| i as i32)
        .unwrap_or(-1);
    let previous_rank = previous_items
        .iter()
        .position(|t| key_func(t) == key_func(item))
        .map(|i| i as i32)
        .unwrap_or(-1);

    let is_new = previous_rank == -1;
    let trend = if is_new {
        "rising"
    } else if current_rank == -1 {
        "falling"
    } else if current_rank < previous_rank {
        "rising"
    } else if current_rank > previous_rank {
        "falling"
    } else {
        "stable"
    };

    json!({"trend": trend, "is_new": is_new})
}

fn calculate_track_trend(
    track: &Track,
    current_tracks: &[Track],
    previous_tracks: &[Track],
) -> Value {
    calculate_trend(track, current_tracks, previous_tracks, |t| {
        t.trackhash.clone()
    })
}

fn calculate_artist_trend(
    artist: &ArtistPeriod,
    current_artists: &[ArtistPeriod],
    previous_artists: &[ArtistPeriod],
) -> Value {
    calculate_trend(artist, current_artists, previous_artists, |a| {
        a.artisthash.clone()
    })
}

fn calculate_album_trend(
    album: &Album,
    current_albums: &[Album],
    previous_albums: &[Album],
) -> Value {
    calculate_trend(album, current_albums, previous_albums, |a| {
        a.albumhash.clone()
    })
}

fn calculate_scrobble_trend(current: i32, previous: i32) -> String {
    if current > previous {
        "rising".to_string()
    } else if current < previous {
        "falling".to_string()
    } else {
        "stable".to_string()
    }
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

fn format_date_range(start: i64, end: i64) -> String {
    let start_dt = DateTime::<Utc>::from_timestamp(start, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());
    let end_dt = DateTime::<Utc>::from_timestamp(end, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());
    format!(
        "{} - {}",
        start_dt.format("%b %-d, %Y"),
        end_dt.format("%b %-d, %Y")
    )
}

fn get_date_range(duration: &str) -> (i64, i64) {
    let now = Utc::now().timestamp();
    let start = match duration {
        "week" => start_of_week(),
        "month" => start_of_month(),
        "year" => start_of_year(),
        "alltime" => 0,
        _ => start_of_year(),
    };
    (start, now)
}

fn get_duration_in_seconds(duration: &str) -> i64 {
    match duration {
        "week" => start_of_week(),
        "month" => start_of_month(),
        "year" => start_of_year(),
        "alltime" => Utc::now().timestamp(),
        _ => start_of_year(),
    }
}

#[derive(Debug, Clone)]
struct ArtistPeriod {
    artisthash: String,
    artist: String,
    playcount: i32,
    playduration: i32,
    tracks: HashMap<String, i32>,
}

async fn get_tracks_in_period(user_id: i64, start: i64, end: i64) -> (Vec<Track>, i32, i32) {
    let scrobbles = ScrobbleTable::get_in_range(user_id, start, end)
        .await
        .unwrap_or_default();

    let mut tracks: HashMap<String, Track> = HashMap::new();
    let mut duration = 0;
    let mut total = 0;

    for scrobble in scrobbles {
        total += 1;
        duration += scrobble.duration;

        if let Some(mut track) = TrackStore::get().get_by_hash(&scrobble.trackhash) {
            let entry = tracks.entry(scrobble.trackhash.clone()).or_insert_with(|| {
                track.playcount = 0;
                track.playduration = 0;
                track
            });
            entry.playcount += 1;
            entry.playduration += scrobble.duration;
        }
    }

    (tracks.into_values().collect(), total, duration)
}

async fn get_artists_in_period(user_id: i64, start: i64, end: i64) -> Vec<ArtistPeriod> {
    let scrobbles = ScrobbleTable::get_in_range(user_id, start, end)
        .await
        .unwrap_or_default();

    let mut artists: HashMap<String, ArtistPeriod> = HashMap::new();

    for scrobble in scrobbles {
        if let Some(track) = TrackStore::get().get_by_hash(&scrobble.trackhash) {
            for artist in track.artists {
                let entry =
                    artists
                        .entry(artist.artisthash.clone())
                        .or_insert_with(|| ArtistPeriod {
                            artisthash: artist.artisthash.clone(),
                            artist: artist.name.clone(),
                            playcount: 0,
                            playduration: 0,
                            tracks: HashMap::new(),
                        });

                entry.playcount += 1;
                entry.playduration += scrobble.duration;
                let track_entry = entry.tracks.entry(track.trackhash.clone()).or_insert(0);
                *track_entry += 1;
            }
        }
    }

    let mut list: Vec<ArtistPeriod> = artists.into_values().collect();
    list.sort_by(|a, b| b.playduration.cmp(&a.playduration));
    list
}

async fn get_albums_in_period(user_id: i64, start: i64, end: i64) -> Vec<Album> {
    let scrobbles = ScrobbleTable::get_in_range(user_id, start, end)
        .await
        .unwrap_or_default();

    let mut albums: HashMap<String, Album> = HashMap::new();

    for scrobble in scrobbles {
        if let Some(track) = TrackStore::get().get_by_hash(&scrobble.trackhash) {
            if let Some(mut album) = AlbumStore::get().get_by_hash(&track.albumhash) {
                let entry = albums.entry(album.albumhash.clone()).or_insert_with(|| {
                    album.playcount = 0;
                    album.playduration = 0;
                    album
                });

                entry.playcount += 1;
                entry.playduration += scrobble.duration;
            }
        }
    }

    albums.into_values().collect()
}

async fn calculate_new_artists(
    current_artists: &[ArtistPeriod],
    timestamp: i64,
    user_id: i64,
) -> usize {
    let current_set: HashSet<String> = current_artists
        .iter()
        .map(|a| a.artisthash.clone())
        .collect();

    let all_records = ScrobbleTable::get_in_range(user_id, 0, timestamp)
        .await
        .unwrap_or_default();
    let trackhashes: HashSet<String> = all_records.into_iter().map(|r| r.trackhash).collect();

    let mut previous_artists_set = HashSet::new();
    let track_store = TrackStore::get();

    for hash in trackhashes {
        if let Some(track) = track_store.get_by_hash(&hash) {
            for artist in track.artists {
                previous_artists_set.insert(artist.artisthash);
            }
        }
    }

    current_set.difference(&previous_artists_set).count()
}

fn calculate_new_albums(current_albums: &[Album], previous_albums: &[Album]) -> usize {
    let current_set: HashSet<String> = current_albums.iter().map(|a| a.albumhash.clone()).collect();
    let previous_set: HashSet<String> = previous_albums
        .iter()
        .map(|a| a.albumhash.clone())
        .collect();

    current_set.difference(&previous_set).count()
}

fn serialize_track_for_stats(track: &Track) -> Map<String, Value> {
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
        Value::Bool(track.is_favorite(DEFAULT_USER_ID)),
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
        "pos",
        "score",
    ] {
        map.remove(key);
    }

    for artist in ["artists", "albumartists"] {
        if let Some(Value::Array(items)) = map.get_mut(artist) {
            for item in items {
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("image");
                }
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
