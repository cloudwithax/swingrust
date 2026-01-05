//! Image processing functions - caching thumbnails and extracting colors

use anyhow::Result;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

use crate::config::{Paths, LG_THUMB_SIZE, MD_THUMB_SIZE, SM_THUMB_SIZE, XSM_THUMB_SIZE};
use crate::core::Tagger;
use crate::stores::{AlbumStore, TrackStore};

/// Cache album images from embedded track art (or nearby folder images) during scans
/// Uses parallel processing for maximum speed (matching Python's multiprocessing approach)
pub async fn cache_album_images() -> Result<usize> {
    let paths = Paths::get()?;

    // Define all thumbnail sizes to generate (matching Python upstream)
    let sizes: [(&str, u32); 4] = [
        ("large", LG_THUMB_SIZE),   // 512px
        ("medium", MD_THUMB_SIZE),  // 256px
        ("small", SM_THUMB_SIZE),   // 96px
        ("xsmall", XSM_THUMB_SIZE), // 64px
    ];

    // Collect unique albums (first track per albumhash)
    let all_tracks = TrackStore::get().get_all();
    let mut seen = std::collections::HashSet::new();
    let albums_to_process: Vec<_> = all_tracks
        .into_iter()
        .filter(|track| {
            if seen.contains(&track.albumhash) {
                return false;
            }
            seen.insert(track.albumhash.clone());
            // Skip if already processed
            let large_dest = paths
                .thumbnails_dir("large")
                .join(format!("{}.webp", track.albumhash));
            !large_dest.exists()
        })
        .collect();

    let total = albums_to_process.len();
    if total > 0 {
        info!(
            "cache_album_images: Processing {} albums in parallel",
            total
        );
    }

    let written = AtomicUsize::new(0);
    let paths_ref = &paths;
    let sizes_ref = &sizes;

    // Process albums in parallel using rayon
    albums_to_process.par_iter().for_each(|track| {
        let path = std::path::Path::new(&track.filepath);

        // Try to read embedded cover first, then folder image
        let cover_bytes = Tagger::read_cover(path)
            .ok()
            .flatten()
            .or_else(|| find_folder_image(path));

        if let Some(data) = cover_bytes {
            if let Ok(img) = image::load_from_memory(&data) {
                let (orig_width, orig_height) = (img.width(), img.height());
                let ratio = orig_width as f32 / orig_height as f32;
                let albumhash = &track.albumhash;

                // Generate all 4 sizes in parallel
                sizes_ref.par_iter().for_each(|(size_name, max_size)| {
                    let dest = paths_ref
                        .thumbnails_dir(size_name)
                        .join(format!("{}.webp", albumhash));

                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }

                    let target_width = (*max_size).min(orig_width);
                    let target_height = (target_width as f32 / ratio) as u32;

                    let resized = img.resize(
                        target_width,
                        target_height,
                        image::imageops::FilterType::Triangle,
                    );
                    let mut buf = Vec::new();
                    if resized
                        .write_to(
                            &mut std::io::Cursor::new(&mut buf),
                            image::ImageFormat::WebP,
                        )
                        .is_ok()
                    {
                        let _ = std::fs::write(&dest, buf);
                    }
                });

                written.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let final_count = written.load(Ordering::Relaxed);
    if final_count > 0 {
        info!(
            "cache_album_images complete: {} album covers cached",
            final_count
        );
    }

    Ok(final_count)
}

fn find_folder_image(track_path: &std::path::Path) -> Option<Vec<u8>> {
    let folder = track_path.parent()?;
    let mut images: Vec<std::path::PathBuf> = std::fs::read_dir(folder)
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

/// Extract dominant colors from album thumbnails and store in database
pub async fn extract_album_colors() -> Result<usize> {
    use crate::db::DbEngine;

    let paths = Paths::get()?;
    let db = DbEngine::get()?;

    // Get existing colors from database
    let existing: std::collections::HashSet<String> = sqlx::query_as::<_, (String,)>(
        "SELECT hash FROM libdata WHERE type = 'album' AND color IS NOT NULL AND color != ''",
    )
    .fetch_all(db.pool())
    .await?
    .into_iter()
    .map(|(h,)| h)
    .collect();

    // Get albums that need color extraction
    let albums_needing_colors: Vec<_> = AlbumStore::get()
        .get_all()
        .into_iter()
        .filter(|album| !existing.contains(&album.albumhash) && album.color.is_empty())
        .collect();

    if albums_needing_colors.is_empty() {
        return Ok(0);
    }

    info!(
        "extract_album_colors: Processing {} albums",
        albums_needing_colors.len()
    );

    let processed = AtomicUsize::new(0);
    let paths_ref = &paths;

    // Extract colors in parallel
    let color_results: Vec<(String, String)> = albums_needing_colors
        .par_iter()
        .filter_map(|album| {
            // Use small thumbnail for color extraction (faster)
            let thumb_path = paths_ref
                .thumbnails_dir("small")
                .join(format!("{}.webp", album.albumhash));

            if !thumb_path.exists() {
                return None;
            }

            // Extract dominant color
            let color = extract_dominant_color(&thumb_path)?;
            processed.fetch_add(1, Ordering::Relaxed);
            Some((album.albumhash.clone(), color))
        })
        .collect();

    // Store colors in database and update in-memory store
    for (albumhash, color) in &color_results {
        // Insert or update in database
        sqlx::query(
            "INSERT INTO libdata (hash, type, color) VALUES (?, 'album', ?) 
             ON CONFLICT(hash) DO UPDATE SET color = excluded.color",
        )
        .bind(albumhash)
        .bind(color)
        .execute(db.pool())
        .await?;

        // Update in-memory store
        AlbumStore::get().set_color(albumhash, color);
    }

    let count = color_results.len();
    if count > 0 {
        info!("extract_album_colors: Extracted {} album colors", count);
    }

    Ok(count)
}

/// Extract the dominant color from an image file
fn extract_dominant_color(path: &std::path::Path) -> Option<String> {
    let img = image::open(path).ok()?;
    let rgb = img.to_rgb8();

    let (width, height) = (rgb.width(), rgb.height());
    if width == 0 || height == 0 {
        return None;
    }

    // Sample pixels and calculate average color
    let sample_step = ((width * height) / 1000).max(1) as usize;
    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;

    for (i, pixel) in rgb.pixels().enumerate() {
        if i % sample_step == 0 {
            r_sum += pixel[0] as u64;
            g_sum += pixel[1] as u64;
            b_sum += pixel[2] as u64;
            count += 1;
        }
    }

    if count == 0 {
        return None;
    }

    let r = (r_sum / count) as u8;
    let g = (g_sum / count) as u8;
    let b = (b_sum / count) as u8;

    Some(format!("rgb({}, {}, {})", r, g, b))
}

// ============== Artist Image Functions ==============

/// Artist image sizes (matching Python upstream)
const LG_ARTIST_IMG_SIZE: u32 = 500; // Original/large
const MD_ARTIST_IMG_SIZE: u32 = 256;
const SM_ARTIST_IMG_SIZE: u32 = 96;

/// Download artist images from Deezer API for artists without images
pub async fn download_artist_images() -> Result<usize> {
    use crate::stores::ArtistStore;

    let paths = Paths::get()?;

    // Get list of existing artist images (check all size directories)
    let mut existing: std::collections::HashSet<String> = std::collections::HashSet::new();

    for size in &["small", "medium", "large"] {
        let artist_path = paths.artist_images_dir(size);
        let _ = std::fs::create_dir_all(&artist_path);

        if let Ok(entries) = std::fs::read_dir(&artist_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    existing.insert(stem.to_string());
                }
            }
        }
    }

    tracing::debug!(
        "download_artist_images: Found {} existing artist images in cache",
        existing.len()
    );

    // Get artists that need images
    let all_artists = ArtistStore::get().get_all();
    let total_artists = all_artists.len();

    // Debug: Show sample of existing hashes vs artist hashes
    if !existing.is_empty() && !all_artists.is_empty() {
        let sample_existing: Vec<_> = existing.iter().take(3).collect();
        let sample_artists: Vec<_> = all_artists.iter().take(3).map(|a| &a.artisthash).collect();
        tracing::debug!(
            "Sample existing: {:?}, Sample artist hashes: {:?}",
            sample_existing,
            sample_artists
        );
    }

    let artists_needing_images: Vec<_> = all_artists
        .into_iter()
        .filter(|artist| !existing.contains(&artist.artisthash))
        .collect();

    if artists_needing_images.is_empty() {
        if !existing.is_empty() {
            info!(
                "download_artist_images: {} artist images already cached, all {} artists covered",
                existing.len(),
                total_artists
            );
        }
        return Ok(0);
    }

    info!(
        "download_artist_images: Fetching images for {} artists from Deezer ({} already cached)",
        artists_needing_images.len(),
        existing.len()
    );

    let mut downloaded = 0usize;
    let mut not_found = 0usize;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Process artists sequentially with small delays to avoid rate limiting
    for artist in &artists_needing_images {
        match fetch_and_save_artist_image(&client, &paths, &artist.name, &artist.artisthash).await {
            Ok(true) => {
                downloaded += 1;
                // Update artist image in store
                ArtistStore::get()
                    .set_image(&artist.artisthash, &format!("{}.webp", artist.artisthash));
            }
            Ok(false) => {
                // Artist not found on Deezer - create a marker file so we don't retry
                let marker_path = paths
                    .artist_images_dir("small")
                    .join(format!("{}.notfound", artist.artisthash));
                let _ = std::fs::write(&marker_path, "");
                not_found += 1;
            }
            Err(e) => {
                tracing::debug!("Failed to fetch image for {}: {}", artist.name, e);
            }
        }

        // Small delay to avoid rate limiting (100ms between requests)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    if downloaded > 0 || not_found > 0 {
        info!(
            "download_artist_images: Downloaded {} artist images, {} not found on Deezer",
            downloaded, not_found
        );
    }

    Ok(downloaded)
}

/// Fetch artist image URL from Deezer API and save it
async fn fetch_and_save_artist_image(
    client: &reqwest::Client,
    paths: &Paths,
    artist_name: &str,
    artist_hash: &str,
) -> Result<bool> {
    use crate::utils::hashing::create_hash;

    // Query Deezer API - reqwest handles URL encoding automatically with query()
    let response = client
        .get("https://api.deezer.com/search/artist")
        .query(&[("q", artist_name)])
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header("Accept", "application/json")
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(false);
    }

    let data: serde_json::Value = response.json().await?;

    // Find matching artist in results
    let results = data["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No data array"))?;

    if results.is_empty() {
        return Ok(false);
    }

    let mut image_url: Option<String> = None;

    // First try exact hash match
    for result in results {
        let result_name = result["name"].as_str().unwrap_or("");
        let result_hash = create_hash(&[result_name], true);

        if result_hash == artist_hash {
            image_url = result["picture_big"].as_str().map(|s| s.to_string());
            break;
        }
    }

    // Fallback: if no exact match, use the first result (likely the best match from Deezer)
    if image_url.is_none() {
        if let Some(first_result) = results.first() {
            image_url = first_result["picture_big"].as_str().map(|s| s.to_string());
        }
    }

    let Some(img_url) = image_url else {
        return Ok(false);
    };

    // Download the image
    let img_response = client.get(&img_url).send().await?;
    if !img_response.status().is_success() {
        return Ok(false);
    }

    let img_bytes = img_response.bytes().await?;
    let img = image::load_from_memory(&img_bytes)?;

    // Save in 3 sizes
    let sizes = [
        ("large", LG_ARTIST_IMG_SIZE),
        ("medium", MD_ARTIST_IMG_SIZE),
        ("small", SM_ARTIST_IMG_SIZE),
    ];

    let (orig_width, orig_height) = (img.width(), img.height());
    let ratio = orig_width as f32 / orig_height as f32;

    for (size_name, max_size) in &sizes {
        let dest = paths
            .artist_images_dir(size_name)
            .join(format!("{}.webp", artist_hash));

        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let target_width = (*max_size).min(orig_width);
        let target_height = (target_width as f32 / ratio) as u32;

        let resized = img.resize(
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        );
        let mut buf = Vec::new();
        if resized
            .write_to(
                &mut std::io::Cursor::new(&mut buf),
                image::ImageFormat::WebP,
            )
            .is_ok()
        {
            let _ = std::fs::write(&dest, buf);
        }
    }

    Ok(true)
}

/// Extract dominant colors from artist images and store in database
pub async fn extract_artist_colors() -> Result<usize> {
    use crate::db::DbEngine;
    use crate::stores::ArtistStore;

    let paths = Paths::get()?;
    let db = DbEngine::get()?;

    // Get existing colors from database
    let existing: std::collections::HashSet<String> = sqlx::query_as::<_, (String,)>(
        "SELECT hash FROM libdata WHERE type = 'artist' AND color IS NOT NULL AND color != ''",
    )
    .fetch_all(db.pool())
    .await?
    .into_iter()
    .map(|(h,)| h)
    .collect();

    // Get artists that need color extraction
    let artists_needing_colors: Vec<_> = ArtistStore::get()
        .get_all()
        .into_iter()
        .filter(|artist| !existing.contains(&artist.artisthash) && artist.color.is_empty())
        .collect();

    if artists_needing_colors.is_empty() {
        return Ok(0);
    }

    info!(
        "extract_artist_colors: Processing {} artists",
        artists_needing_colors.len()
    );

    let processed = AtomicUsize::new(0);
    let paths_ref = &paths;

    // Extract colors in parallel
    let color_results: Vec<(String, String)> = artists_needing_colors
        .par_iter()
        .filter_map(|artist| {
            // Use small artist image for color extraction
            let img_path = paths_ref
                .artist_images_dir("small")
                .join(format!("{}.webp", artist.artisthash));

            if !img_path.exists() {
                return None;
            }

            // Extract dominant color
            let color = extract_dominant_color(&img_path)?;
            processed.fetch_add(1, Ordering::Relaxed);
            Some((artist.artisthash.clone(), color))
        })
        .collect();

    // Store colors in database and update in-memory store
    for (artisthash, color) in &color_results {
        // Insert or update in database
        sqlx::query(
            "INSERT INTO libdata (hash, type, color) VALUES (?, 'artist', ?) 
             ON CONFLICT(hash) DO UPDATE SET color = excluded.color",
        )
        .bind(artisthash)
        .bind(color)
        .execute(db.pool())
        .await?;

        // Update in-memory store
        ArtistStore::get().set_color(artisthash, color);
    }

    let count = color_results.len();
    if count > 0 {
        info!("extract_artist_colors: Extracted {} artist colors", count);
    }

    Ok(count)
}
