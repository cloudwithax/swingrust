//! SwingMusic - A beautiful, self-hosted music player for your local audio files
//!
//! This is a 1:1 Rust rewrite of the Python SwingMusic application.

#![allow(dead_code)]
#![allow(unused_variables)]

mod api;
mod config;
mod core;
mod db;
mod models;
mod plugins;
mod serializers;
mod stores;
mod utils;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;

/// SwingMusic - Self-hosted music player
#[derive(Parser, Debug)]
#[command(name = "swingmusic")]
#[command(author = "swingmx")]
#[command(version = "2.0.0")]
#[command(about = "A beautiful, self-hosted music player for your local audio files")]
struct Args {
    /// Host address to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 1970)]
    port: u16,

    /// Enable debug mode
    #[arg(long)]
    debug: bool,

    /// Path to config directory
    #[arg(long)]
    config: Option<PathBuf>,

    /// Path to web client
    #[arg(long)]
    client: Option<PathBuf>,

    /// Provide a JSON setup file to skip interactive prompts
    #[arg(long)]
    setup_config: Option<PathBuf>,

    /// Reset password for a user
    #[arg(long)]
    password_reset: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // initialize logging with filters to suppress noisy dependency warnings
    let log_level = if args.debug { "debug" } else { "info" };

    // filter out noisy warnings from audio parsing libraries
    let filter = tracing_subscriber::EnvFilter::new(format!(
        "{},symphonia=error,symphonia_core=error,symphonia_bundle_mp3=error,lofty=error",
        log_level
    ));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    info!("SwingMusic v2.0.0 starting...");

    // Initialize paths
    let paths = config::Paths::init(args.config, args.client)?;
    info!("Config directory: {:?}", paths.config_dir());

    // Handle password reset mode
    if args.password_reset {
        return utils::tools::password_reset().await;
    }

    // Setup and run
    start_swingmusic(args.host, args.port, args.setup_config).await
}

async fn start_swingmusic(host: String, port: u16, setup_config: Option<PathBuf>) -> Result<()> {
    // Run setup
    info!("Running setup...");
    run_setup(setup_config).await?;

    // Ensure ffmpeg/ffprobe are available (download if needed)
    info!("Checking ffmpeg availability...");
    if let Err(e) = core::ffmpeg::ensure_ffmpeg() {
        tracing::warn!("Failed to ensure ffmpeg: {}. Transcoding may not work.", e);
    } else {
        info!("ffmpeg is available");
    }

    // Ensure we have an initial library scan before loading stores
    // We run this in the background so the server can start immediately
    info!("Checking for initial library scan...");

    // log the resolved root dirs so operators can verify their mounts
    {
        let cfg = config::UserConfig::load()?;
        if cfg.root_dirs.is_empty() {
            tracing::warn!(
                "No music root directories configured. \
                 Set SWING_ROOT_DIRS or configure via the web UI."
            );
        } else {
            info!("Music root directories: {:?}", cfg.root_dirs);
            for dir in &cfg.root_dirs {
                let p = std::path::Path::new(dir);
                if !p.is_dir() {
                    tracing::warn!(
                        "Root directory '{}' does not exist or is not accessible. \
                         Is the volume mounted?",
                        dir
                    );
                }
            }
        }
    }

    tokio::spawn(async {
        if let Err(e) = maybe_run_initial_scan().await {
            tracing::error!("Initial scan error: {}", e);
        }
    });

    // Build the application
    info!("Building application...");

    // Load data into memory
    info!("Loading data into memory...");
    load_into_memory().await?;

    // Start background tasks
    info!("Starting background tasks...");
    start_background_tasks().await?;

    // Start the server
    let addr = format!("{}:{}", host, port);
    info!("Server listening on http://{}", addr);

    use actix_cors::Cors;
    use actix_web::{middleware, App, HttpServer};

    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .configure(api::configure)
    })
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}

async fn run_setup(setup_config: Option<PathBuf>) -> Result<()> {
    use crate::config::UserConfig;
    use crate::db::{run_migrations, setup_sqlite, setup_userdata, UserTable};
    use crate::utils::tools::{apply_setup_file, configure_root_dirs_from_env, interactive_setup};

    // Setup config file
    let mut config = UserConfig::load()?;

    // Generate server ID if missing
    if config.server_id.is_empty() {
        config.server_id = uuid::Uuid::new_v4().to_string();
        config.save()?;
    }

    // Setup main database
    setup_sqlite().await?;

    // Setup userdata database (for similar artists, etc.)
    setup_userdata().await?;

    // Run migrations
    run_migrations().await?;

    // always sync root directories from the SWING_ROOT_DIRS env var.
    // this MUST happen on every startup (not just first-run) so that docker
    // users can change the env var between restarts and have it take effect.
    if let Err(e) = configure_root_dirs_from_env() {
        tracing::warn!("Failed to configure root dirs from environment: {}", e);
    }

    // Apply setup file or interactive prompts when no users exist
    if let Some(path) = setup_config {
        info!("Applying setup from file: {:?}", path);
        apply_setup_file(&path).await?;
    } else {
        let users = UserTable::all().await?;
        if users.is_empty() {
            interactive_setup().await?;
        }
    }

    Ok(())
}

/// Run a one-time library scan on first startup so media is available immediately
async fn maybe_run_initial_scan() -> Result<()> {
    use crate::config::UserConfig;
    use crate::core::indexer::Indexer;
    use crate::db::tables::TrackTable;

    // Skip when tracks already exist (subsequent starts)
    let existing_tracks = TrackTable::count().await?;
    if existing_tracks > 0 {
        info!("Library already indexed ({} tracks)", existing_tracks);
        return Ok(());
    }

    let config = UserConfig::load()?;
    if config.root_dirs.is_empty() {
        info!("No music root directories configured; skipping initial scan");
        return Ok(());
    }

    info!("Running initial library scan...");
    let indexer = Indexer::from_config(&config).with_progress(false);
    let tracks = indexer.index()?;

    if tracks.is_empty() {
        info!("Initial scan found no audio files in configured roots");
        return Ok(());
    }

    TrackTable::insert_many(&tracks).await?;
    info!("Initial scan indexed {} tracks", tracks.len());

    // Reload stores to make tracks available immediately
    load_into_memory().await?;

    Ok(())
}

async fn load_into_memory() -> Result<()> {
    use crate::core::images::{
        cache_album_images, download_artist_images, extract_album_colors, extract_artist_colors,
    };
    use crate::core::mapstuff::{map_colors, map_favorites, map_scrobble_data};
    use crate::stores::{AlbumStore, ArtistStore, FolderStore, TrackStore};

    // Load tracks
    info!("Loading tracks...");
    TrackStore::load_all_tracks().await?;

    // Load albums
    info!("Loading albums...");
    AlbumStore::load_albums().await?;

    // Load artists
    info!("Loading artists...");
    ArtistStore::load_artists().await?;

    // Load folder paths
    info!("Loading folder paths...");
    FolderStore::load_filepaths().await?;

    // Initialize file serving cache (for fast file lookups and http caching)
    info!("Initializing file serving cache...");
    crate::core::file_cache::init_file_cache().await?;

    // Cache album images (extract from tracks)
    info!("Caching album images...");
    if let Ok(cached) = cache_album_images().await {
        if cached > 0 {
            info!("Cached {} album covers", cached);
        }
    }

    // Extract album colors
    info!("Extracting album colors...");
    let _ = extract_album_colors().await;

    // Download artist images from Deezer (run in background to not block startup)
    info!("Downloading artist images...");
    let _ = download_artist_images().await;

    // Extract artist colors
    info!("Extracting artist colors...");
    let _ = extract_artist_colors().await;

    // Map additional data
    info!("Mapping favorites...");
    map_favorites().await?;

    info!("Mapping colors...");
    map_colors().await?;

    info!("Mapping scrobble data...");
    map_scrobble_data().await?;

    Ok(())
}

async fn start_background_tasks() -> Result<()> {
    use crate::plugins::register_plugins;

    // Register plugins
    register_plugins().await?;

    // Start cron jobs
    tokio::spawn(async {
        if let Err(e) = crate::core::crons::start_cron_jobs().await {
            tracing::error!("Cron jobs error: {}", e);
        }
    });

    // Start file watcher if enabled
    let config = crate::config::UserConfig::load()?;
    if config.enable_watchdog {
        tokio::spawn(async {
            if let Err(e) = crate::core::watchdogg::start_watchdog().await {
                tracing::error!("Watchdog error: {}", e);
            }
        });
    }

    Ok(())
}
