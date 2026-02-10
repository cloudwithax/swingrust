//! file serving cache - optimizations for high-performance file delivery
//!
//! provides caching layers for:
//! - validated root directories (avoid config parsing per request)
//! - file metadata (etags, modification times)
//! - filepath resolution (trackhash -> filepath mapping)
//! - memory-mapped file regions for small files

use dashmap::DashMap;
use lru::LruCache;
use memmap2::Mmap;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::fs::{File, Metadata};
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::config::UserConfig;
use crate::utils::filesystem::normalize_path;

// threshold for memory-mapped serving (files under this size are mmap'd)
const MMAP_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024; // 50mb

// max number of mmap'd files to keep in cache
const MMAP_CACHE_SIZE: usize = 100;

// max number of file metadata entries to cache
const METADATA_CACHE_SIZE: usize = 1000;

// global cache instance
static FILE_CACHE: OnceCell<Arc<FileCache>> = OnceCell::new();

/// cached file metadata for etag generation and conditional requests
#[derive(Clone, Debug)]
pub struct CachedFileMetadata {
    pub size: u64,
    pub modified: SystemTime,
    pub etag: String,
}

impl CachedFileMetadata {
    pub fn from_metadata(metadata: &Metadata) -> io::Result<Self> {
        let size = metadata.len();
        let modified = metadata.modified()?;
        
        // generate etag from size + modification time
        let modified_secs = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let etag = format!("\"{:x}-{:x}\"", modified_secs, size);
        
        Ok(Self {
            size,
            modified,
            etag,
        })
    }
    
    /// format modification time as http-date for last-modified header
    pub fn last_modified_http(&self) -> String {
        use chrono::{DateTime, Utc};
        
        let datetime: DateTime<Utc> = self.modified.into();
        datetime.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }
}

/// memory-mapped file region for zero-copy serving
pub struct MmapRegion {
    pub mmap: Mmap,
    pub metadata: CachedFileMetadata,
}

/// validated filepath resolution result
#[derive(Clone)]
pub struct ResolvedPath {
    pub filepath: PathBuf,
    pub content_type: String,
    pub filename: String,
}

/// high-performance file serving cache
pub struct FileCache {
    /// pre-computed normalized root directories
    root_dirs: Vec<String>,
    
    /// home directory for $home substitution
    home_dir: String,
    
    /// cached file metadata (path -> metadata)
    metadata_cache: Mutex<LruCache<PathBuf, CachedFileMetadata>>,
    
    /// memory-mapped file cache for small files
    mmap_cache: Mutex<LruCache<PathBuf, Arc<MmapRegion>>>,
    
    /// validated filepath resolution cache (trackhash -> resolved path)
    resolution_cache: DashMap<String, ResolvedPath>,
    
    /// trackhash -> filepath quick lookup (avoids full Track clone)
    filepath_index: DashMap<String, String>,
}

impl FileCache {
    /// initialize the global file cache
    pub fn init() -> anyhow::Result<Arc<Self>> {
        let cache = FILE_CACHE.get_or_init(|| {
            let config = UserConfig::load().unwrap_or_default();
            
            let home_dir = directories::UserDirs::new()
                .map(|u| normalize_path(&u.home_dir().to_string_lossy()))
                .unwrap_or_default();
            
            // pre-compute normalized root directories
            let root_dirs: Vec<String> = config
                .root_dirs
                .iter()
                .map(|root| {
                    if root == "$home" {
                        home_dir.clone()
                    } else {
                        normalize_path(root)
                    }
                })
                .filter(|r| !r.is_empty())
                .collect();
            
            Arc::new(Self {
                root_dirs,
                home_dir,
                metadata_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(METADATA_CACHE_SIZE).unwrap(),
                )),
                mmap_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(MMAP_CACHE_SIZE).unwrap(),
                )),
                resolution_cache: DashMap::new(),
                filepath_index: DashMap::new(),
            })
        });
        
        Ok(cache.clone())
    }
    
    /// get the global cache instance
    pub fn get() -> Option<Arc<Self>> {
        FILE_CACHE.get().cloned()
    }
    
    /// reload root directories from config (call after config changes)
    pub fn reload_config(&self) -> anyhow::Result<()> {
        // note: this is a no-op for now since root_dirs is not mutable
        // to properly support dynamic config reloading, we'd need interior mutability
        Ok(())
    }
    
    /// check if a path is within allowed root directories
    pub fn is_path_allowed(&self, filepath: &str) -> bool {
        let normalized = normalize_path(filepath);
        self.root_dirs.iter().any(|root| normalized.starts_with(root))
    }
    
    /// register a trackhash -> filepath mapping for quick lookup
    pub fn register_filepath(&self, trackhash: &str, filepath: &str) {
        self.filepath_index
            .insert(trackhash.to_string(), filepath.to_string());
    }
    
    /// bulk register filepaths from track store
    pub fn register_filepaths(&self, mappings: impl Iterator<Item = (String, String)>) {
        for (hash, path) in mappings {
            self.filepath_index.insert(hash, path);
        }
    }
    
    /// get cached filepath for trackhash
    pub fn get_filepath(&self, trackhash: &str) -> Option<String> {
        self.filepath_index.get(trackhash).map(|v| v.clone())
    }
    
    /// cache a resolved path for future requests
    pub fn cache_resolution(&self, trackhash: &str, resolved: ResolvedPath) {
        self.resolution_cache.insert(trackhash.to_string(), resolved);
    }
    
    /// get cached resolution for trackhash
    pub fn get_resolution(&self, trackhash: &str) -> Option<ResolvedPath> {
        self.resolution_cache.get(trackhash).map(|v| v.clone())
    }
    
    /// invalidate resolution cache for a trackhash (call when file changes)
    pub fn invalidate_resolution(&self, trackhash: &str) {
        self.resolution_cache.remove(trackhash);
    }
    
    /// get or compute file metadata with caching
    pub fn get_metadata(&self, path: &Path) -> io::Result<CachedFileMetadata> {
        // check cache first
        {
            let mut cache = self.metadata_cache.lock();
            if let Some(cached) = cache.get(&path.to_path_buf()) {
                return Ok(cached.clone());
            }
        }
        
        // compute fresh metadata
        let metadata = std::fs::metadata(path)?;
        let cached = CachedFileMetadata::from_metadata(&metadata)?;
        
        // store in cache
        {
            let mut cache = self.metadata_cache.lock();
            cache.put(path.to_path_buf(), cached.clone());
        }
        
        Ok(cached)
    }
    
    /// invalidate cached metadata for a path (call when file changes)
    pub fn invalidate_metadata(&self, path: &Path) {
        let mut cache = self.metadata_cache.lock();
        cache.pop(&path.to_path_buf());
    }
    
    /// get memory-mapped file if under threshold, with caching
    pub fn get_mmap(&self, path: &Path) -> io::Result<Option<Arc<MmapRegion>>> {
        // check cache first
        {
            let mut cache = self.mmap_cache.lock();
            if let Some(region) = cache.get(&path.to_path_buf()) {
                return Ok(Some(region.clone()));
            }
        }
        
        // check file size
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > MMAP_THRESHOLD_BYTES {
            return Ok(None); // too large for mmap
        }
        
        // create mmap
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let cached_meta = CachedFileMetadata::from_metadata(&metadata)?;
        
        let region = Arc::new(MmapRegion {
            mmap,
            metadata: cached_meta,
        });
        
        // store in cache
        {
            let mut cache = self.mmap_cache.lock();
            cache.put(path.to_path_buf(), region.clone());
        }
        
        Ok(Some(region))
    }
    
    /// invalidate mmap cache for a path
    pub fn invalidate_mmap(&self, path: &Path) {
        let mut cache = self.mmap_cache.lock();
        cache.pop(&path.to_path_buf());
    }
    
    /// invalidate all caches for a path (call when file is modified/deleted)
    pub fn invalidate_path(&self, path: &Path) {
        self.invalidate_metadata(path);
        self.invalidate_mmap(path);
    }
    
    /// clear all caches
    pub fn clear(&self) {
        self.metadata_cache.lock().clear();
        self.mmap_cache.lock().clear();
        self.resolution_cache.clear();
    }
}

/// check if client's conditional request can be satisfied with 304 response
pub fn check_conditional_request(
    if_none_match: Option<&str>,
    if_modified_since: Option<&str>,
    metadata: &CachedFileMetadata,
) -> bool {
    // check etag first (stronger validator)
    if let Some(client_etag) = if_none_match {
        // handle comma-separated etags and wildcard
        if client_etag == "*" {
            return true;
        }
        for etag in client_etag.split(',') {
            let etag = etag.trim();
            // strip weak prefix if present
            let etag = etag.strip_prefix("W/").unwrap_or(etag);
            if etag == metadata.etag {
                return true;
            }
        }
    }
    
    // check if-modified-since as fallback
    if let Some(ims) = if_modified_since {
        if let Ok(client_time) = parse_http_date(ims) {
            let file_time = metadata
                .modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            // file not modified if client time >= file time
            if client_time >= file_time {
                return true;
            }
        }
    }
    
    false
}

/// parse http date format (rfc 7231)
fn parse_http_date(date_str: &str) -> Result<u64, ()> {
    use chrono::{DateTime, Utc};
    
    // try rfc 2822 format first (most common)
    if let Ok(dt) = DateTime::parse_from_rfc2822(date_str) {
        return Ok(dt.timestamp() as u64);
    }
    
    // try rfc 3339 format
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Ok(dt.timestamp() as u64);
    }
    
    // try http date format: "Sun, 06 Nov 1994 08:49:37 GMT"
    if let Ok(dt) = DateTime::parse_from_str(date_str, "%a, %d %b %Y %H:%M:%S GMT") {
        return Ok(dt.timestamp() as u64);
    }
    
    // fallback: try parsing with chrono's more lenient parser
    if let Ok(dt) = date_str.parse::<DateTime<Utc>>() {
        return Ok(dt.timestamp() as u64);
    }
    
    Err(())
}

/// initialize the file cache (call during startup)
pub async fn init_file_cache() -> anyhow::Result<()> {
    FileCache::init()?;
    
    // pre-populate filepath index from track store
    if let Some(cache) = FileCache::get() {
        use crate::stores::TrackStore;
        
        let store = TrackStore::get();
        let tracks = store.get_all();
        
        cache.register_filepaths(
            tracks
                .into_iter()
                .map(|t| (t.trackhash, t.filepath)),
        );
        
        tracing::info!(
            "file cache initialized with {} filepath mappings",
            cache.filepath_index.len()
        );
    }
    
    Ok(())
}
