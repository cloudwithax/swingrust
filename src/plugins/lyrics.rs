//! lyrics plugins for musixmatch and spotify color lyrics

use anyhow::{anyhow, Context, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::warn;

use crate::config::Paths;

const MUSIXMATCH_ROOT_URL: &str = "https://apic-desktop.musixmatch.com/ws/1.1/";
const SPOTIFY_TOKEN_URL: &str = "https://open.spotify.com/api/token";
const SPOTIFY_LYRICS_URL: &str = "https://spclient.wg.spotify.com/color-lyrics/v2/track/";
const SPOTIFY_SERVER_TIME_URL: &str = "https://open.spotify.com/api/server-time";
const SPOTIFY_SECRET_URL: &str = "https://raw.githubusercontent.com/xyloflake/spot-secrets-go/refs/heads/main/secrets/secretDict.json";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Cached token with expiration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedToken {
    token: String,
    expiration_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpotifyTokenCache {
    token: String,
    expires_at_ms: u128,
}

#[derive(Debug, Clone, Deserialize)]
struct SpotifyTokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "accessTokenExpirationTimestampMs")]
    expires_at_ms: u64,
    #[serde(default, rename = "isAnonymous")]
    is_anonymous: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SpotifyLyricsResponse {
    lyrics: Option<SpotifyLyricsBody>,
}

#[derive(Debug, Deserialize)]
struct SpotifyLyricsBody {
    #[serde(rename = "syncType", default)]
    sync_type: Option<String>,
    #[serde(default)]
    lines: Vec<SpotifyLyricsLine>,
}

#[derive(Debug, Deserialize)]
struct SpotifyLyricsLine {
    #[serde(rename = "startTimeMs")]
    start_time_ms: Option<String>,
    words: Option<String>,
}

/// Lyrics search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsSearchResult {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub image: String,
}

/// Musixmatch lyrics provider
pub struct MusixmatchProvider {
    client: Client,
    token: Arc<RwLock<Option<String>>>,
}

impl MusixmatchProvider {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_default();

        Self {
            client,
            token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get current timestamp
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Load cached token from file
    async fn load_cached_token(&self) -> Option<String> {
        let paths = Paths::get().ok()?;
        let token_path = paths.lyrics_plugins_dir().join("lyrics_token.json");

        if token_path.exists() {
            let content = std::fs::read_to_string(&token_path).ok()?;
            let cached: CachedToken = serde_json::from_str(&content).ok()?;

            if Self::now() < cached.expiration_time {
                return Some(cached.token);
            }
        }
        None
    }

    /// Save token to cache file
    async fn save_token(&self, token: &str) -> Result<()> {
        let paths = Paths::get()?;
        let token_path = paths.lyrics_plugins_dir().join("lyrics_token.json");

        let cached = CachedToken {
            token: token.to_string(),
            expiration_time: Self::now() + 600, // 10 minutes
        };

        let content = serde_json::to_string(&cached)?;
        std::fs::write(&token_path, content)?;
        Ok(())
    }

    /// Get or fetch token
    async fn get_token(&self) -> Result<String> {
        // Check memory cache
        {
            let token = self.token.read().await;
            if let Some(t) = token.as_ref() {
                return Ok(t.clone());
            }
        }

        // Check file cache
        if let Some(cached) = self.load_cached_token().await {
            let mut token = self.token.write().await;
            *token = Some(cached.clone());
            return Ok(cached);
        }

        // Fetch new token
        let new_token = self.fetch_token().await?;

        // Cache it
        {
            let mut token = self.token.write().await;
            *token = Some(new_token.clone());
        }
        self.save_token(&new_token).await?;

        Ok(new_token)
    }

    /// Fetch new token from Musixmatch
    fn fetch_token(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let t = Self::now() * 1000;
            let url = format!(
                "{}token.get?user_language=en&app_id=web-desktop-app-v1.0&t={}",
                MUSIXMATCH_ROOT_URL, t
            );

            let resp = self
                .client
                .get(&url)
                .header("authority", "apic-desktop.musixmatch.com")
                .header("cookie", "AWSELBCORS=0; AWSELB=0")
                .send()
                .await?;

            let json: serde_json::Value = resp.json().await?;

            let status = json["message"]["header"]["status_code"]
                .as_i64()
                .unwrap_or(0);

            if status == 401 {
                // Rate limited, wait and retry
                tokio::time::sleep(tokio::time::Duration::from_secs(13)).await;
                return self.fetch_token().await;
            }

            json["message"]["body"]["user_token"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow!("Failed to get token"))
        })
    }

    /// Make authenticated request
    async fn get(&self, action: &str, params: &[(&str, &str)]) -> Result<serde_json::Value> {
        let token = self.get_token().await?;
        let t = Self::now() * 1000;

        let mut query: Vec<(&str, String)> =
            params.iter().map(|(k, v)| (*k, v.to_string())).collect();

        query.push(("app_id", "web-desktop-app-v1.0".to_string()));
        query.push(("usertoken", token));
        query.push(("t", t.to_string()));

        let url = format!("{}{}", MUSIXMATCH_ROOT_URL, action);

        let resp = self
            .client
            .get(&url)
            .query(&query)
            .header("authority", "apic-desktop.musixmatch.com")
            .header("cookie", "AWSELBCORS=0; AWSELB=0")
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;
        Ok(json)
    }

    /// Search for lyrics by title and artist
    pub async fn search(&self, title: &str, artist: &str) -> Result<Vec<LyricsSearchResult>> {
        // Try original artist first, then decoded version
        let artist_variants = vec![artist.to_string(), deunicode::deunicode(artist)];

        for artist_name in artist_variants {
            let json = self
                .get(
                    "track.search",
                    &[
                        ("q_track", title),
                        ("q_artist", &artist_name),
                        ("page_size", "5"),
                        ("page", "1"),
                        ("f_has_lyrics", "1"),
                        ("s_track_rating", "desc"),
                        ("quorum_factor", "1.0"),
                    ],
                )
                .await?;

            let tracks = json["message"]["body"]["track_list"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            if !tracks.is_empty() {
                let results: Vec<LyricsSearchResult> = tracks
                    .iter()
                    .filter_map(|t| {
                        let track = &t["track"];
                        Some(LyricsSearchResult {
                            track_id: track["track_id"].as_i64()?.to_string(),
                            title: track["track_name"].as_str()?.to_string(),
                            artist: track["artist_name"].as_str()?.to_string(),
                            album: track["album_name"].as_str().unwrap_or("").to_string(),
                            image: track["album_coverart_100x100"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect();
                return Ok(results);
            }
        }

        Ok(Vec::new())
    }

    /// Get LRC lyrics by track ID
    pub async fn get_lrc_by_id(&self, track_id: &str) -> Result<Option<String>> {
        let json = self
            .get(
                "track.subtitle.get",
                &[("track_id", track_id), ("subtitle_format", "lrc")],
            )
            .await?;

        let body = &json["message"]["body"];
        if body.is_null() {
            return Ok(None);
        }

        let lyrics = body["subtitle"]["subtitle_body"]
            .as_str()
            .map(|s| s.to_string());

        Ok(lyrics)
    }

    /// Download lyrics and save to file
    pub async fn download_lyrics(&self, track_id: &str, filepath: &str) -> Result<Option<String>> {
        let lrc = self.get_lrc_by_id(track_id).await?;

        if let Some(ref lyrics) = lrc {
            if lyrics.replace('\n', "").trim().is_empty() {
                return Ok(None);
            }

            let lrc_path = std::path::Path::new(filepath).with_extension("lrc");
            std::fs::write(&lrc_path, lyrics)?;
        }

        Ok(lrc)
    }
}

impl Default for MusixmatchProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// spotify color lyrics provider
pub struct SpotifyLyricsProvider {
    client: Client,
    sp_dc: Option<String>,
    token: Arc<RwLock<Option<SpotifyTokenCache>>>,
}

impl SpotifyLyricsProvider {
    pub fn new(sp_dc: Option<String>) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        Self {
            client,
            sp_dc,
            token: Arc::new(RwLock::new(None)),
        }
    }

    fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    fn token_path() -> Option<PathBuf> {
        Paths::get()
            .ok()
            .map(|paths| paths.lyrics_plugins_dir().join("spotify_token.json"))
    }

    pub fn is_configured(&self) -> bool {
        self.sp_dc.as_ref().is_some()
    }

    async fn load_cached_token(&self) -> Option<SpotifyTokenCache> {
        let path = Self::token_path()?;
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(path).ok()?;
        let cache: SpotifyTokenCache = serde_json::from_str(&content).ok()?;

        if cache.expires_at_ms > Self::now_ms() {
            Some(cache)
        } else {
            None
        }
    }

    async fn save_token(&self, cache: &SpotifyTokenCache) -> Result<()> {
        let path = Self::token_path().context("paths not initialized")?;
        let content = serde_json::to_string(cache)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    async fn get_token(&self) -> Result<String> {
        if let Some(token) = self.token.read().await.as_ref() {
            if token.expires_at_ms > Self::now_ms() {
                return Ok(token.token.clone());
            }
        }

        if let Some(cached) = self.load_cached_token().await {
            if cached.expires_at_ms > Self::now_ms() {
                let mut token = self.token.write().await;
                *token = Some(cached.clone());
                return Ok(cached.token);
            }
        }

        let fetched = self.fetch_token().await?;
        {
            let mut token = self.token.write().await;
            *token = Some(fetched.clone());
        }
        self.save_token(&fetched).await?;
        Ok(fetched.token)
    }

    async fn fetch_token(&self) -> Result<SpotifyTokenCache> {
        let sp_dc = self
            .sp_dc
            .as_ref()
            .ok_or_else(|| anyhow!("spotify sp_dc cookie is not configured"))?;

        let params = self.get_server_time_params().await?;
        let response = self
            .client
            .get(SPOTIFY_TOKEN_URL)
            .query(&params)
            .header("user-agent", USER_AGENT)
            .header("cookie", format!("sp_dc={}", sp_dc))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "failed to fetch spotify token status {}",
                response.status()
            ));
        }

        let payload: SpotifyTokenResponse = response.json().await?;
        if payload.is_anonymous.unwrap_or(false) {
            return Err(anyhow!("spotify rejected sp_dc cookie"));
        }

        let cache = SpotifyTokenCache {
            token: payload.access_token,
            expires_at_ms: payload.expires_at_ms as u128,
        };

        Ok(cache)
    }

    async fn latest_secret(&self) -> Result<(Vec<u8>, String)> {
        let response = self
            .client
            .get(SPOTIFY_SECRET_URL)
            .header("user-agent", USER_AGENT)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("failed to fetch spotify secret map"));
        }

        let data: serde_json::Value = response.json().await?;
        let map = data
            .as_object()
            .ok_or_else(|| anyhow!("spotify secret payload is not an object"))?;

        let (version, values) = map
            .iter()
            .max_by_key(|(k, _)| k.parse::<u32>().unwrap_or(0))
            .ok_or_else(|| anyhow!("no spotify secret versions found"))?;

        let arr = values
            .as_array()
            .ok_or_else(|| anyhow!("spotify secret version is not an array"))?;

        let mut secret = Vec::with_capacity(arr.len());
        for (idx, value) in arr.iter().enumerate() {
            let num = value
                .as_i64()
                .ok_or_else(|| anyhow!("spotify secret value is not a number"))?
                as u8;
            let transformed = num ^ (((idx as u8) % 33) + 9);
            secret.push(transformed);
        }

        Ok((secret, version.clone()))
    }

    fn generate_totp(secret: &[u8], server_time_seconds: u64) -> Result<String> {
        type HmacSha1 = Hmac<Sha1>;

        let counter = server_time_seconds / 30;
        let mut mac = HmacSha1::new_from_slice(secret)?;
        mac.update(&counter.to_be_bytes());
        let result = mac.finalize().into_bytes();
        let offset = (result[result.len() - 1] & 0x0f) as usize;

        let binary = ((result[offset] & 0x7f) as u32) << 24
            | ((result[offset + 1] & 0xff) as u32) << 16
            | ((result[offset + 2] & 0xff) as u32) << 8
            | ((result[offset + 3] & 0xff) as u32);

        let code = binary % 1_000_000;
        Ok(format!("{:06}", code))
    }

    async fn get_server_time_params(&self) -> Result<Vec<(String, String)>> {
        let response = self
            .client
            .get(SPOTIFY_SERVER_TIME_URL)
            .header("user-agent", USER_AGENT)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("failed to fetch spotify server time"));
        }

        let body: serde_json::Value = response.json().await?;
        let server_time = body["serverTime"]
            .as_u64()
            .ok_or_else(|| anyhow!("spotify server time missing"))?;

        let (secret, version) = self.latest_secret().await?;
        let totp = Self::generate_totp(&secret, server_time)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        Ok(vec![
            ("reason".to_string(), "transport".to_string()),
            ("productType".to_string(), "web-player".to_string()),
            ("totp".to_string(), totp),
            ("totpVer".to_string(), version),
            ("ts".to_string(), timestamp),
        ])
    }

    pub async fn get_lrc_by_id(&self, track_id: &str) -> Result<Option<String>> {
        let token = self.get_token().await?;
        let url = format!(
            "{}{}?format=json&market=from_token",
            SPOTIFY_LYRICS_URL, track_id
        );

        let response = self
            .client
            .get(&url)
            .header("user-agent", USER_AGENT)
            .header("app-platform", "WebPlayer")
            .bearer_auth(token)
            .send()
            .await?;

        if !response.status().is_success() {
            warn!("spotify lyrics request failed status={}", response.status());
            return Ok(None);
        }

        let payload: SpotifyLyricsResponse = response.json().await?;
        let Some(body) = payload.lyrics else {
            return Ok(None);
        };

        if body.lines.is_empty() {
            return Ok(None);
        }

        let mut lrc = String::new();
        for line in body.lines {
            let text = line.words.unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }

            let start_ms: u64 = line
                .start_time_ms
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            let time_tag = format_lrc_time(start_ms);
            lrc.push_str(&format!("[{}]{}\n", time_tag, text));
        }

        if lrc.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(lrc))
    }

    pub async fn download_lyrics(&self, track_id: &str, filepath: &str) -> Result<Option<String>> {
        let lrc = self.get_lrc_by_id(track_id).await?;

        if let Some(ref lyrics) = lrc {
            if lyrics.replace('\n', "").trim().is_empty() {
                return Ok(None);
            }

            let lrc_path = PathBuf::from(filepath).with_extension("lrc");
            std::fs::write(&lrc_path, lyrics)?;
        }

        Ok(lrc)
    }
}

impl Default for SpotifyLyricsProvider {
    fn default() -> Self {
        Self::new(find_sp_dc_cookie())
    }
}

fn find_sp_dc_cookie() -> Option<String> {
    if let Ok(val) = std::env::var("SP_DC") {
        if !val.trim().is_empty() {
            return Some(val);
        }
    }

    if let Some(paths) = Paths::get().ok() {
        let candidate = paths.config_dir().join(".env");
        if let Some(val) = read_sp_dc_from_file(&candidate) {
            return Some(val);
        }
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let candidate = cwd.join(".env");
    read_sp_dc_from_file(&candidate)
}

fn read_sp_dc_from_file(path: &std::path::Path) -> Option<String> {
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            if key.trim() == "SP_DC" {
                let v = value.trim().trim_matches('"');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }

    None
}

fn format_lrc_time(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    let centiseconds = (ms % 1000) / 10;
    format!("{:02}:{:02}.{:02}", minutes, seconds, centiseconds)
}

/// Lyrics plugin
pub struct LyricsPlugin {
    pub provider: MusixmatchProvider,
    pub spotify: Option<SpotifyLyricsProvider>,
    pub enabled: bool,
}

impl LyricsPlugin {
    pub fn new() -> Self {
        Self {
            provider: MusixmatchProvider::new(),
            spotify: {
                let provider = SpotifyLyricsProvider::default();
                if provider.is_configured() {
                    Some(provider)
                } else {
                    None
                }
            },
            enabled: true,
        }
    }

    pub async fn search(&self, title: &str, artist: &str) -> Result<Vec<LyricsSearchResult>> {
        if !self.enabled {
            return Err(anyhow!("Lyrics plugin is disabled"));
        }
        self.provider.search(title, artist).await
    }

    pub async fn download(&self, track_id: &str, filepath: &str) -> Result<Option<String>> {
        if !self.enabled {
            return Err(anyhow!("Lyrics plugin is disabled"));
        }
        self.provider.download_lyrics(track_id, filepath).await
    }

    pub async fn download_spotify(&self, track_id: &str, filepath: &str) -> Result<Option<String>> {
        if !self.enabled {
            return Err(anyhow!("Lyrics plugin is disabled"));
        }

        let provider = self
            .spotify
            .as_ref()
            .ok_or_else(|| anyhow!("spotify lyrics requires sp_dc to be set"))?;

        provider.download_lyrics(track_id, filepath).await
    }

    pub fn spotify_configured(&self) -> bool {
        self.spotify
            .as_ref()
            .map(|p| p.is_configured())
            .unwrap_or(false)
    }
}

impl Default for LyricsPlugin {
    fn default() -> Self {
        Self::new()
    }
}
