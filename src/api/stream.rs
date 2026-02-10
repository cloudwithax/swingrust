//! Audio streaming API routes

use actix_files::NamedFile;
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use serde::Deserialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use crate::config::UserConfig;
use crate::core::silence::SilenceDetector;
use crate::core::transcode::{AudioFormat, Quality, Transcoder};
use crate::stores::TrackStore;
use crate::utils::filesystem::normalize_path;

/// Stream query parameters
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub format: Option<String>,
    pub quality: Option<String>,
}

/// Legacy stream query parameters (filepath passthrough, no ranges)
#[derive(Debug, Deserialize)]
pub struct LegacyStreamQuery {
    pub filepath: String,
    pub quality: Option<String>,
    pub container: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SilenceBody {
    pub ending_file: String,
    pub starting_file: String,
}

/// Stream track by hash
#[get("/{trackhash}")]
pub async fn stream_track(
    path: web::Path<String>,
    query: web::Query<StreamQuery>,
    req: HttpRequest,
) -> impl Responder {
    let trackhash = path.into_inner();

    // Find track
    let track = match TrackStore::get().get_by_hash(&trackhash) {
        Some(t) => t,
        None => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Track not found"
            }));
        }
    };

    let file_path = Path::new(&track.filepath);

    if !file_path.exists() {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "Track file not found"
        }));
    }

    // determine quality from query param (shared across explicit and auto transcode)
    let quality = match query.quality.as_deref() {
        Some("low") => Quality::Low,
        Some("medium") => Quality::Medium,
        Some("high") => Quality::High,
        Some("best") => Quality::Best,
        _ => Quality::Best,
    };

    // explicit transcode request via ?format=xxx
    if let Some(format_str) = &query.format {
        if let Some(format) = AudioFormat::from_str(format_str) {
            match Transcoder::transcode_to_bytes(file_path, format, quality) {
                Ok(data) => {
                    return HttpResponse::Ok()
                        .content_type(format.mime_type())
                        .body(data);
                }
                Err(e) => {
                    tracing::error!("transcoding failed: {}", e);
                    // fall through to auto-transcode or raw serving
                }
            }
        }
    }

    // auto-transcode for formats browsers can't play natively
    // (wma, aiff, alac, ape, wv, mpc, dsf, dff, tta, etc.)
    let file_ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if !AudioFormat::is_browser_compatible(file_ext) {
        let target = AudioFormat::default_transcode_target();
        tracing::debug!(
            "auto-transcoding {} ({}) -> {}",
            track.trackhash,
            file_ext,
            target.extension()
        );

        match Transcoder::transcode_to_bytes(file_path, target, quality) {
            Ok(data) => {
                return HttpResponse::Ok()
                    .content_type(target.mime_type())
                    .body(data);
            }
            Err(e) => {
                tracing::error!("auto-transcode failed for {}: {}", file_path.display(), e);
                // last resort: serve raw file and hope the client can deal with it
            }
        }
    }

    // serve original file with range request support (browser-compatible formats)
    serve_file_with_ranges(file_path, &req).await
}

/// Serve file with HTTP range request support
async fn serve_file_with_ranges(file_path: &Path, req: &HttpRequest) -> HttpResponse {
    let file = match std::fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to open file"),
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to get file metadata"),
    };

    let file_size = metadata.len();

    // determine content type using centralized extension -> mime mapping
    let content_type = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(AudioFormat::mime_type_for_extension)
        .unwrap_or("application/octet-stream");

    // Check for Range header
    if let Some(range_header) = req.headers().get("Range") {
        let range_str = range_header.to_str().unwrap_or("");

        if let Some(range) = parse_range(range_str, file_size) {
            let (start, end) = range;
            let length = end - start + 1;

            let mut file = file;
            if file.seek(SeekFrom::Start(start)).is_err() {
                return HttpResponse::InternalServerError().body("Failed to seek in file");
            }

            let mut buffer = vec![0u8; length as usize];
            if file.read_exact(&mut buffer).is_err() {
                return HttpResponse::InternalServerError().body("Failed to read file");
            }

            return HttpResponse::PartialContent()
                .insert_header(("Content-Type", content_type))
                .insert_header(("Content-Length", length.to_string()))
                .insert_header((
                    "Content-Range",
                    format!("bytes {}-{}/{}", start, end, file_size),
                ))
                .insert_header(("Accept-Ranges", "bytes"))
                .body(buffer);
        }
    }

    // Serve full file
    match NamedFile::open(file_path) {
        Ok(named_file) => named_file.into_response(req),
        Err(_) => HttpResponse::InternalServerError().body("Failed to serve file"),
    }
}

/// Parse HTTP Range header
fn parse_range(range_header: &str, file_size: u64) -> Option<(u64, u64)> {
    if !range_header.starts_with("bytes=") {
        return None;
    }

    let range_spec = &range_header[6..];
    let parts: Vec<&str> = range_spec.split('-').collect();

    if parts.len() != 2 {
        return None;
    }

    let start = if parts[0].is_empty() {
        // Range like "-500" means last 500 bytes
        let suffix_length: u64 = parts[1].parse().ok()?;
        file_size.saturating_sub(suffix_length)
    } else {
        parts[0].parse().ok()?
    };

    let end = if parts[1].is_empty() {
        file_size - 1
    } else {
        parts[1].parse::<u64>().ok()?.min(file_size - 1)
    };

    if start <= end && start < file_size {
        Some((start, end))
    } else {
        None
    }
}

/// Get track info for streaming
#[get("/{trackhash}/info")]
pub async fn stream_info(path: web::Path<String>) -> impl Responder {
    let trackhash = path.into_inner();

    let track = match TrackStore::get().get_by_hash(&trackhash) {
        Some(t) => t,
        None => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Track not found"
            }));
        }
    };

    let file_path = Path::new(&track.filepath);
    let file_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

    let content_type = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(AudioFormat::mime_type_for_extension)
        .unwrap_or("application/octet-stream");

    HttpResponse::Ok().json(serde_json::json!({
        "trackhash": track.trackhash,
        "title": track.title,
        "artist": track.artist(),
        "album": track.album,
        "duration": track.duration,
        "bitrate": track.bitrate,
        "samplerate": 0, // Not stored in track model
        "content_type": content_type,
        "file_size": file_size,
        "supports_range": true
    }))
}

/// Legacy file endpoint used by upstream clients (no range / transcoding)
///
/// optimizations applied:
/// - http caching headers (etag, last-modified, cache-control)
/// - conditional request handling (304 not modified)
/// - memory-mapped file serving for smaller files
/// - cached path resolution to avoid repeated lookups
/// - pre-computed root directory validation
/// - x-sendfile header support for reverse proxies
#[get("/{trackhash}/legacy")]
pub async fn stream_track_legacy(
    path: web::Path<String>,
    query: web::Query<LegacyStreamQuery>,
    req: HttpRequest,
) -> impl Responder {
    use crate::core::file_cache::{FileCache, ResolvedPath};

    let requested_hash = path.into_inner();
    let raw_filepath = query.filepath.clone();

    // basic path traversal guard
    let path_buf = PathBuf::from(&raw_filepath);
    if path_buf
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Invalid filepath",
            "error": "Path traversal detected"
        }));
    }

    // try to get file cache for optimized path validation and serving
    let file_cache = FileCache::get();

    // check for cached resolution first (fastest path)
    if let Some(ref cache) = file_cache {
        if let Some(resolved) = cache.get_resolution(&requested_hash) {
            // verify file still exists (could have been deleted)
            if resolved.filepath.exists() {
                return serve_file_optimized(
                    &resolved.filepath,
                    &resolved.content_type,
                    &resolved.filename,
                    cache,
                    &req,
                )
                .await;
            } else {
                // file was deleted, invalidate cache
                cache.invalidate_resolution(&requested_hash);
            }
        }
    }

    // use cached root directory check if available, otherwise fallback to config
    let path_allowed = if let Some(ref cache) = file_cache {
        cache.is_path_allowed(&raw_filepath)
    } else {
        ensure_in_root_dirs(&raw_filepath).is_ok()
    };

    if !path_allowed {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Invalid filepath",
            "error": "File not inside root directories"
        }));
    }

    // resolve track filepath using lightweight lookup
    let store = TrackStore::get();

    // try path lookup first (avoids full track clone when possible)
    let filepath = store
        .get_by_path(&raw_filepath)
        .filter(|t| t.trackhash == requested_hash)
        .map(|t| t.filepath.clone())
        .or_else(|| store.get_filepath_by_hash(&requested_hash));

    let Some(filepath) = filepath else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "msg": "File Not Found"
        }));
    };

    let file_path = PathBuf::from(&filepath);
    if !file_path.exists() {
        return HttpResponse::NotFound().json(serde_json::json!({
            "msg": "File Not Found"
        }));
    }

    let content_type = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("track")
        .to_string();

    // cache the resolution for future requests
    if let Some(ref cache) = file_cache {
        cache.cache_resolution(
            &requested_hash,
            ResolvedPath {
                filepath: file_path.clone(),
                content_type: content_type.clone(),
                filename: filename.clone(),
            },
        );
    }

    // serve with optimizations
    if let Some(ref cache) = file_cache {
        serve_file_optimized(&file_path, &content_type, &filename, cache, &req).await
    } else {
        // fallback to basic serving when cache not available
        serve_file_basic(&file_path, &content_type, &filename, &req)
    }
}

/// serve file with all optimizations: etag, last-modified, conditional requests, mmap
async fn serve_file_optimized(
    file_path: &Path,
    content_type: &str,
    filename: &str,
    cache: &std::sync::Arc<crate::core::file_cache::FileCache>,
    req: &HttpRequest,
) -> HttpResponse {
    use crate::core::file_cache::check_conditional_request;

    // get cached metadata for etag/last-modified
    let metadata = match cache.get_metadata(file_path) {
        Ok(m) => m,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to read file metadata"
            }));
        }
    };

    // check conditional request headers for 304 response
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|v| v.to_str().ok());
    let if_modified_since = req
        .headers()
        .get("If-Modified-Since")
        .and_then(|v| v.to_str().ok());

    if check_conditional_request(if_none_match, if_modified_since, &metadata) {
        return HttpResponse::NotModified()
            .insert_header(("ETag", metadata.etag.as_str()))
            .insert_header(("Last-Modified", metadata.last_modified_http()))
            .finish();
    }

    // check for x-sendfile support (reverse proxy optimization)
    // if the x-sendfile-type header is present, delegate to nginx/apache
    if let Some(sendfile_type) = req.headers().get("X-Sendfile-Type") {
        if let Ok(sendfile_type) = sendfile_type.to_str() {
            let path_str = file_path.to_string_lossy();
            let header_name = match sendfile_type {
                "X-Accel-Redirect" => "X-Accel-Redirect", // nginx
                _ => "X-Sendfile",                        // apache/lighttpd
            };

            return HttpResponse::Ok()
                .insert_header((header_name, path_str.as_ref()))
                .insert_header(("Content-Type", content_type))
                .insert_header(("ETag", metadata.etag.as_str()))
                .insert_header(("Last-Modified", metadata.last_modified_http()))
                .insert_header(("Cache-Control", "private, max-age=31536000"))
                .insert_header((
                    "Content-Disposition",
                    format!("attachment; filename=\"{}\"", filename),
                ))
                .finish();
        }
    }

    // try memory-mapped serving for small files
    if let Ok(Some(mmap_region)) = cache.get_mmap(file_path) {
        let data = mmap_region.mmap.to_vec(); // copy from mmap
        return HttpResponse::Ok()
            .insert_header(("Content-Type", content_type))
            .insert_header(("Content-Length", data.len().to_string()))
            .insert_header(("ETag", metadata.etag.as_str()))
            .insert_header(("Last-Modified", metadata.last_modified_http()))
            .insert_header(("Cache-Control", "private, max-age=31536000"))
            .insert_header(("Accept-Ranges", "bytes"))
            .insert_header((
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", filename),
            ))
            .body(data);
    }

    // fallback to namedfile for large files (handles range requests efficiently)
    serve_file_with_caching_headers(file_path, content_type, filename, &metadata, req)
}

/// serve file with caching headers using namedfile
fn serve_file_with_caching_headers(
    file_path: &Path,
    content_type: &str,
    filename: &str,
    metadata: &crate::core::file_cache::CachedFileMetadata,
    req: &HttpRequest,
) -> HttpResponse {
    match actix_files::NamedFile::open(file_path) {
        Ok(file) => {
            let mut response = file
                .set_content_disposition(actix_web::http::header::ContentDisposition {
                    disposition: actix_web::http::header::DispositionType::Attachment,
                    parameters: vec![actix_web::http::header::DispositionParam::Filename(
                        filename.to_string(),
                    )],
                })
                .into_response(req);

            // add caching headers
            let headers = response.headers_mut();
            headers.insert(
                actix_web::http::header::ETAG,
                metadata.etag.parse().unwrap(),
            );
            headers.insert(
                actix_web::http::header::LAST_MODIFIED,
                metadata.last_modified_http().parse().unwrap(),
            );
            headers.insert(
                actix_web::http::header::CACHE_CONTROL,
                "private, max-age=31536000".parse().unwrap(),
            );

            response
        }
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to open file"
        })),
    }
}

/// basic file serving without cache optimizations (fallback)
fn serve_file_basic(
    file_path: &Path,
    content_type: &str,
    filename: &str,
    req: &HttpRequest,
) -> HttpResponse {
    match actix_files::NamedFile::open(file_path) {
        Ok(file) => file
            .set_content_disposition(actix_web::http::header::ContentDisposition {
                disposition: actix_web::http::header::DispositionType::Attachment,
                parameters: vec![actix_web::http::header::DispositionParam::Filename(
                    filename.to_string(),
                )],
            })
            .into_response(req),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to open file"
        })),
    }
}

/// get silence paddings between two files (milliseconds)
#[post("/silence")]
pub async fn get_audio_silence(body: web::Json<SilenceBody>) -> impl Responder {
    let ending_file = Path::new(&body.ending_file);
    let starting_file = Path::new(&body.starting_file);

    if body.ending_file.is_empty() || body.starting_file.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"msg": "No filepath provided"}));
    }

    let ending_info = SilenceDetector::detect(ending_file);
    let starting_info = SilenceDetector::detect(starting_file);

    let (silence_end, silence_start) = match (ending_info, starting_info) {
        (Ok(end), Ok(start)) => {
            let end_ms = ((end.duration - end.silence_end) * 1000.0).round() as i64;
            let start_ms = (start.silence_start * 1000.0).round() as i64;
            (end_ms.max(0), start_ms.max(0))
        }
        _ => (0, 0),
    };

    HttpResponse::Ok().json(serde_json::json!({
        "ending": silence_end,
        "starting": silence_start
    }))
}

fn ensure_in_root_dirs(raw_filepath: &str) -> Result<(), HttpResponse> {
    let config = UserConfig::load().map_err(|e| {
        HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": format!("Failed to load config: {}", e)
        }))
    })?;

    let home_dir = directories::UserDirs::new()
        .map(|u| normalize_path(&u.home_dir().to_string_lossy()))
        .unwrap_or_default();
    let normalized_filepath = normalize_path(raw_filepath);
    let mut allowed = false;
    for root in &config.root_dirs {
        let normalized_root = if root == "$home" {
            home_dir.clone()
        } else {
            normalize_path(root)
        };

        if !normalized_root.is_empty() && normalized_filepath.starts_with(&normalized_root) {
            allowed = true;
            break;
        }
    }

    if !allowed {
        return Err(HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Invalid filepath",
            "error": "File not inside root directories"
        })));
    }

    Ok(())
}

/// Configure stream routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(stream_track).service(stream_info);
}

/// Configure legacy file routes (upstream compatibility)
pub fn configure_file(cfg: &mut web::ServiceConfig) {
    cfg.service(stream_track_legacy).service(get_audio_silence);
}
