//! REST API routes for SwingMusic

pub mod album;
pub mod artist;
pub mod auth;
pub mod backup;
pub mod collections;
pub mod colors;
pub mod favorites;
pub mod folder;
pub mod getall;
pub mod home;
pub mod imgserver;
pub mod logger;
pub mod lyrics;
pub mod playlist;
pub mod plugins;
pub mod plugins_mixes;
pub mod scrobble;
pub mod search;
pub mod settings;
pub mod stream;
pub mod track;

use actix_web::web;

/// Configure all API routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // Album routes
        .service(web::scope("/album").configure(album::configure))
        // Artist routes
        .service(web::scope("/artist").configure(artist::configure))
        // Auth routes
        .service(web::scope("/auth").configure(auth::configure))
        // Backup routes
        .service(web::scope("/backup").configure(backup::configure))
        // Collection routes
        .service(web::scope("/collections").configure(collections::configure))
        // Colors routes
        .service(web::scope("/colors").configure(colors::configure))
        // Favorites routes
        .service(web::scope("/favorites").configure(favorites::configure))
        // Folder routes
        .service(web::scope("/folder").configure(folder::configure))
        // GetAll routes (for getting all tracks/albums/artists)
        .service(web::scope("/getall").configure(getall::configure))
        // Home routes
        .service(web::scope("/home").configure(home::configure))
        // Home routes (upstream prefix)
        .service(web::scope("/nothome").configure(home::configure_upstream))
        // Image server routes
        .service(web::scope("/img").configure(imgserver::configure))
        // Lyrics routes
        .service(web::scope("/lyrics").configure(lyrics::configure))
        // Playlist routes
        .service(web::scope("/playlist").configure(playlist::configure))
        // Playlist routes (upstream prefix)
        .service(web::scope("/playlists").configure(playlist::configure_upstream))
        // Plugin routes
        .service(web::scope("/plugins").configure(plugins::configure))
        // Mixes plugin routes
        .service(web::scope("/plugins/mixes").configure(plugins_mixes::configure))
        // File routes (upstream legacy stream)
        .service(web::scope("/file").configure(stream::configure_file))
        // Search routes
        .service(web::scope("/search").configure(search::configure))
        // Settings routes
        .service(web::scope("/settings").configure(settings::configure))
        // Settings routes (upstream prefix)
        .service(web::scope("/notsettings").configure(settings::configure_upstream))
        // Stream routes
        .service(web::scope("/stream").configure(stream::configure))
        // Track routes
        .service(web::scope("/track").configure(track::configure))
        // Logger/stats routes
        .service(web::scope("/logger").configure(logger::configure));
}
