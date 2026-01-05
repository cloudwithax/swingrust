//! Colors API routes limited to upstream parity

use actix_web::{get, web, HttpResponse, Responder};

use crate::stores::AlbumStore;

/// Upstream: GET /colors/album/<albumhash>
#[get("/album/{albumhash}")]
pub async fn get_album_color(path: web::Path<String>) -> impl Responder {
    let albumhash = path.into_inner();
    let album = AlbumStore::get().get_by_hash(&albumhash);

    match album {
        Some(a) if !a.color.is_empty() => HttpResponse::Ok().json(serde_json::json!({
            "color": a.color,
        })),
        _ => HttpResponse::NotFound().json(serde_json::json!({ "color": "" })),
    }
}

/// Configure color routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_album_color);
}
