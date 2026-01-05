//! Image server API routes

use actix_files::NamedFile;
use actix_web::{get, web, HttpResponse, Responder};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::config::Paths;
use crate::core::Tagger;
use crate::stores::TrackStore;

/// Image query params
#[derive(Debug, Deserialize)]
pub struct ImageQuery {
    pub w: Option<u32>, // Width
    pub h: Option<u32>, // Height
}

#[derive(Debug, Deserialize)]
pub struct ThumbQuery {
    #[serde(default)]
    pub pathhash: String,
}

#[derive(Clone, Copy)]
struct ThumbSpec {
    size_label: &'static str,
    max_px: u32,
}

impl ThumbSpec {
    fn path<'a>(&self, paths: &'a Paths, name: &str) -> PathBuf {
        paths.thumbnails_dir(self.size_label).join(name)
    }
}

const THUMB_LG: ThumbSpec = ThumbSpec {
    size_label: "large",
    max_px: 512,
};
const THUMB_MD: ThumbSpec = ThumbSpec {
    size_label: "medium",
    max_px: 256,
};
const THUMB_SM: ThumbSpec = ThumbSpec {
    size_label: "small",
    max_px: 96,
};
const THUMB_XS: ThumbSpec = ThumbSpec {
    size_label: "xsmall",
    max_px: 64,
};

/// Get album image
#[get("/album/{hash}")]
pub async fn get_album_image(
    path: web::Path<String>,
    query: web::Query<ImageQuery>,
) -> impl Responder {
    let hash = path.into_inner();
    let paths = match Paths::get() {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Paths not initialized: {e}"))
        }
    };

    // Try different extensions
    for ext in &["webp", "jpg", "jpeg", "png"] {
        let image_path = paths
            .album_images("large")
            .join(format!("{}.{}", hash, ext));

        if image_path.exists() {
            if query.w.is_some() || query.h.is_some() {
                // Resize image
                return serve_resized_image(&image_path, query.w, query.h).await;
            } else {
                return match std::fs::read(&image_path) {
                    Ok(bytes) => HttpResponse::Ok()
                        .content_type(
                            mime_guess::from_path(&image_path)
                                .first_or_octet_stream()
                                .essence_str(),
                        )
                        .body(bytes),
                    Err(_) => HttpResponse::NotFound().body("Image not found"),
                };
            }
        }
    }

    HttpResponse::NotFound().body("Album image not found")
}

/// Get artist image (large)
#[get("/artist/{hash}")]
pub async fn get_artist_image(
    path: web::Path<String>,
    query: web::Query<ImageQuery>,
) -> impl Responder {
    serve_artist_image_size(&path.into_inner(), "large", query.w, query.h).await
}

/// Get small artist image (96px)
#[get("/artist/small/{imgpath}")]
pub async fn get_artist_image_small(path: web::Path<String>) -> impl Responder {
    serve_artist_image_size(&path.into_inner(), "small", None, None).await
}

/// Get medium artist image (256px)
#[get("/artist/medium/{imgpath}")]
pub async fn get_artist_image_medium(path: web::Path<String>) -> impl Responder {
    serve_artist_image_size(&path.into_inner(), "medium", None, None).await
}

/// Helper to serve artist images from a specific size folder
async fn serve_artist_image_size(
    imgpath: &str,
    size: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> HttpResponse {
    let paths = match Paths::get() {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Paths not initialized: {e}"))
        }
    };

    // Extract hash from imgpath (remove extension if present)
    let hash = imgpath
        .trim_end_matches(".webp")
        .trim_end_matches(".jpg")
        .trim_end_matches(".jpeg")
        .trim_end_matches(".png");

    // Try different extensions
    for ext in &["webp", "jpg", "jpeg", "png"] {
        let image_path = paths
            .artist_images_dir(size)
            .join(format!("{}.{}", hash, ext));

        if image_path.exists() {
            if width.is_some() || height.is_some() {
                return serve_resized_image(&image_path, width, height).await;
            } else {
                return match std::fs::read(&image_path) {
                    Ok(bytes) => HttpResponse::Ok()
                        .content_type(
                            mime_guess::from_path(&image_path)
                                .first_or_octet_stream()
                                .essence_str(),
                        )
                        .body(bytes),
                    Err(_) => HttpResponse::NotFound().body("Image not found"),
                };
            }
        }
    }

    // Return fallback image or 404
    HttpResponse::NotFound().body("Artist image not found")
}

/// Get track thumbnail (embedded art)
#[get("/track/{hash}")]
pub async fn get_track_image(
    path: web::Path<String>,
    query: web::Query<ImageQuery>,
) -> impl Responder {
    let hash = path.into_inner();

    // Find track
    let track = match TrackStore::get().get_by_hash(&hash) {
        Some(t) => t,
        None => return HttpResponse::NotFound().body("Track not found"),
    };

    // Try to get embedded cover
    let track_path = std::path::Path::new(&track.filepath);

    match Tagger::read_cover(track_path) {
        Ok(Some(data)) => {
            // Determine mime type from data
            let mime = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
                "image/jpeg"
            } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                "image/png"
            } else {
                "image/webp"
            };

            if query.w.is_some() || query.h.is_some() {
                // Resize
                return serve_resized_bytes(&data, mime, query.w, query.h).await;
            }

            HttpResponse::Ok().content_type(mime).body(data)
        }
        _ => {
            // Fall back to album image
            let paths = match Paths::get() {
                Ok(p) => p,
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .body(format!("Paths not initialized: {e}"))
                }
            };
            let album_image = paths
                .album_images("large")
                .join(format!("{}.webp", track.albumhash));

            if album_image.exists() {
                if query.w.is_some() || query.h.is_some() {
                    return serve_resized_image(&album_image, query.w, query.h).await;
                }

                match std::fs::read(&album_image) {
                    Ok(bytes) => HttpResponse::Ok()
                        .content_type(
                            mime_guess::from_path(&album_image)
                                .first_or_octet_stream()
                                .essence_str(),
                        )
                        .body(bytes),
                    Err(_) => HttpResponse::NotFound().body("Image not found"),
                }
            } else {
                HttpResponse::NotFound().body("No image available")
            }
        }
    }
}

/// Get playlist image
#[get("/playlist/{id}")]
pub async fn get_playlist_image(path: web::Path<i64>) -> impl Responder {
    let id = path.into_inner();
    let paths = match Paths::get() {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Paths not initialized: {e}"))
        }
    };

    for ext in &["webp", "jpg", "jpeg", "png"] {
        let image_path = paths.playlist_images_dir().join(format!("{}.{}", id, ext));

        if image_path.exists() {
            return match std::fs::read(&image_path) {
                Ok(bytes) => HttpResponse::Ok()
                    .content_type(
                        mime_guess::from_path(&image_path)
                            .first_or_octet_stream()
                            .essence_str(),
                    )
                    .body(bytes),
                Err(_) => HttpResponse::NotFound().body("Image not found"),
            };
        }
    }

    HttpResponse::NotFound().body("Playlist image not found")
}

/// Serve resized image from path
async fn serve_resized_image(
    path: &PathBuf,
    width: Option<u32>,
    height: Option<u32>,
) -> HttpResponse {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return HttpResponse::NotFound().body("Failed to read image"),
    };

    let mime = if path.extension().map(|e| e == "png").unwrap_or(false) {
        "image/png"
    } else {
        "image/jpeg"
    };

    serve_resized_bytes(&data, mime, width, height).await
}

/// Serve resized image from bytes
async fn serve_resized_bytes(
    data: &[u8],
    mime: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> HttpResponse {
    let img = match image::load_from_memory(data) {
        Ok(i) => i,
        Err(_) => return HttpResponse::NotFound().body("Failed to load image"),
    };

    let resized = match (width, height) {
        (Some(w), Some(h)) => img.thumbnail_exact(w, h),
        (Some(w), None) => img.thumbnail(w, img.height()),
        (None, Some(h)) => img.thumbnail(img.width(), h),
        (None, None) => img,
    };

    let mut buf = Vec::new();
    let format = if mime == "image/png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };

    if resized
        .write_to(&mut std::io::Cursor::new(&mut buf), format)
        .is_err()
    {
        return HttpResponse::InternalServerError().body("Failed to encode image");
    }

    HttpResponse::Ok().content_type(mime).body(buf)
}

/// Configure image routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_album_image)
        .service(get_artist_image)
        .service(get_artist_image_small)
        .service(get_artist_image_medium)
        .service(get_track_image)
        .service(get_playlist_image)
        .service(get_thumb_large)
        .service(get_thumb_medium)
        .service(get_thumb_small)
        .service(get_thumb_xsmall);
}

// -------- Thumbnail endpoints (upstream-compatible) --------

#[get("/thumbnail/{imgpath}")]
pub async fn get_thumb_large(
    path: web::Path<String>,
    query: web::Query<ThumbQuery>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    serve_or_create_thumb(&path.into_inner(), THUMB_LG, &query.pathhash, &req).await
}

#[get("/thumbnail/medium/{imgpath}")]
pub async fn get_thumb_medium(
    path: web::Path<String>,
    query: web::Query<ThumbQuery>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    serve_or_create_thumb(&path.into_inner(), THUMB_MD, &query.pathhash, &req).await
}

#[get("/thumbnail/small/{imgpath}")]
pub async fn get_thumb_small(
    path: web::Path<String>,
    query: web::Query<ThumbQuery>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    serve_or_create_thumb(&path.into_inner(), THUMB_SM, &query.pathhash, &req).await
}

#[get("/thumbnail/xsmall/{imgpath}")]
pub async fn get_thumb_xsmall(
    path: web::Path<String>,
    query: web::Query<ThumbQuery>,
    req: actix_web::HttpRequest,
) -> impl Responder {
    serve_or_create_thumb(&path.into_inner(), THUMB_XS, &query.pathhash, &req).await
}

async fn serve_or_create_thumb(
    imgname: &str,
    spec: ThumbSpec,
    pathhash: &str,
    req: &actix_web::HttpRequest,
) -> HttpResponse {
    let paths = match Paths::get() {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Paths not initialized: {e}"))
        }
    };

    let target = spec.path(&paths, imgname);
    if target.exists() {
        return serve_named(&target, req).await;
    }

    if let Err(e) = std::fs::create_dir_all(target.parent().unwrap_or(Path::new("."))) {
        return HttpResponse::InternalServerError()
            .body(format!("Failed to prepare cache dir: {e}"));
    }

    // Try to build from existing large image first
    match build_thumb_from_album_image(&paths, imgname, spec.max_px, &target).await {
        Ok(true) => return serve_named(&target, req).await,
        Ok(false) => {}
        Err(_) => {}
    }

    // If no cached large image, try to extract from track using pathhash
    if !pathhash.is_empty() {
        if let Ok(true) =
            extract_thumb_from_track(&paths, imgname, pathhash, spec.max_px, &target).await
        {
            return serve_named(&target, req).await;
        }
    }

    HttpResponse::NotFound().body("Image not found")
}

async fn serve_named(path: &Path, req: &actix_web::HttpRequest) -> HttpResponse {
    match NamedFile::open(path) {
        Ok(file) => file.into_response(req),
        Err(_) => HttpResponse::NotFound().body("Image not found"),
    }
}

async fn build_thumb_from_album_image(
    paths: &Paths,
    imgname: &str,
    max_px: u32,
    target: &Path,
) -> anyhow::Result<bool> {
    let stem = Path::new(imgname)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(imgname);

    // Try common extensions to locate original
    let exts = ["webp", "jpg", "jpeg", "png"];
    let mut source: Option<PathBuf> = None;
    for ext in &exts {
        let candidate = paths
            .album_images("large")
            .join(format!("{}.{}", stem, ext));
        if candidate.exists() {
            source = Some(candidate);
            break;
        }
    }

    let Some(source_path) = source else {
        return Ok(false);
    };

    let data = std::fs::read(&source_path)?;
    let img = image::load_from_memory(&data)?;
    let resized = img.thumbnail(max_px, max_px);

    let mut buf = Vec::new();
    let format = match target.extension().and_then(|e| e.to_str()) {
        Some("png") => image::ImageFormat::Png,
        Some("jpg") | Some("jpeg") => image::ImageFormat::Jpeg,
        _ => image::ImageFormat::WebP,
    };

    resized.write_to(&mut std::io::Cursor::new(&mut buf), format)?;
    std::fs::write(target, buf)?;
    Ok(true)
}

/// Extract thumbnail from track embedded art on-demand
async fn extract_thumb_from_track(
    paths: &Paths,
    imgname: &str,
    pathhash: &str,
    max_px: u32,
    target: &Path,
) -> anyhow::Result<bool> {
    use crate::utils::hashing::create_hash;

    let albumhash = Path::new(imgname)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(imgname);

    // Find a track with matching albumhash and pathhash (folder hash)
    let tracks = TrackStore::get().get_by_album(albumhash);

    let matching_track = tracks.iter().find(|t| {
        let folder_hash = create_hash(&[&t.folder], false);
        folder_hash == pathhash || t.folder.contains(pathhash)
    });

    let track = match matching_track {
        Some(t) => t,
        None => tracks
            .first()
            .ok_or_else(|| anyhow::anyhow!("No track found"))?,
    };

    // Try embedded cover first
    let track_path = Path::new(&track.filepath);
    let cover_bytes = Tagger::read_cover(track_path)
        .ok()
        .flatten()
        .or_else(|| find_folder_image(track_path));

    let Some(data) = cover_bytes else {
        return Ok(false);
    };

    let img = image::load_from_memory(&data)?;
    let resized = img.thumbnail(max_px, max_px);

    let mut buf = Vec::new();
    resized.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::WebP,
    )?;
    std::fs::write(target, buf)?;

    // Also save to large for future requests
    let large_target = paths
        .thumbnails_dir("large")
        .join(format!("{}.webp", albumhash));
    if !large_target.exists() {
        let large_resized = img.thumbnail(512, 512);
        let mut large_buf = Vec::new();
        if large_resized
            .write_to(
                &mut std::io::Cursor::new(&mut large_buf),
                image::ImageFormat::WebP,
            )
            .is_ok()
        {
            let _ = std::fs::write(&large_target, large_buf);
        }
    }

    Ok(true)
}

fn find_folder_image(track_path: &Path) -> Option<Vec<u8>> {
    let folder = track_path.parent()?;
    let mut images: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|ext| {
                        matches!(ext.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp")
                    })
                    .unwrap_or(false)
        })
        .collect();

    if images.is_empty() {
        return None;
    }

    // Prioritize common cover names
    let priority = ["cover", "front", "folder", "album", "artwork", "back"];
    images.sort_by_key(|p| {
        let name = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        priority
            .iter()
            .position(|pref| name.starts_with(pref))
            .unwrap_or(priority.len())
    });

    std::fs::read(images[0].clone()).ok()
}
