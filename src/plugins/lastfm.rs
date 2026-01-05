//! Last.fm Plugin - scrobbles plays to Last.fm

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;

use crate::config::UserConfig;
use crate::models::Track;

const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

/// Last.fm API response
#[derive(Debug, Deserialize)]
struct LastfmResponse {
    #[serde(default)]
    error: Option<i32>,
    #[serde(default)]
    message: Option<String>,
}

/// Last.fm session response
#[derive(Debug, Deserialize)]
struct SessionResponse {
    session: Option<SessionInfo>,
}

#[derive(Debug, Deserialize)]
struct SessionInfo {
    name: String,
    key: String,
}

/// Last.fm plugin for scrobbling
pub struct LastFmPlugin {
    client: Client,
    api_key: String,
    api_secret: String,
    pub enabled: bool,
}

impl LastFmPlugin {
    pub fn new() -> Self {
        let config = UserConfig::load().unwrap_or_default();

        Self {
            client: Client::new(),
            api_key: config.lastfm_api_key.clone(),
            api_secret: config.lastfm_api_secret.clone(),
            enabled: !config.lastfm_api_key.is_empty(),
        }
    }

    pub fn with_credentials(api_key: String, api_secret: String) -> Self {
        let enabled = !api_key.is_empty();
        Self {
            client: Client::new(),
            api_key,
            api_secret,
            enabled,
        }
    }

    /// Generate API signature
    fn generate_signature(&self, params: &BTreeMap<&str, String>) -> String {
        let mut sig_string = String::new();

        for (key, value) in params {
            sig_string.push_str(key);
            sig_string.push_str(value);
        }
        sig_string.push_str(&self.api_secret);

        let mut hasher = Sha1::new();
        hasher.update(sig_string.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Get session key for user (requires user to authenticate via web)
    pub async fn get_session(&self, token: &str) -> Result<(String, String)> {
        let mut params = BTreeMap::new();
        params.insert("method", "auth.getSession".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("token", token.to_string());

        let sig = self.generate_signature(&params);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let resp = self
            .client
            .post(LASTFM_API_URL)
            .form(&params)
            .send()
            .await?;

        let json: SessionResponse = resp.json().await?;

        if let Some(session) = json.session {
            Ok((session.name, session.key))
        } else {
            Err(anyhow!("Failed to get session"))
        }
    }

    /// helper to return only the session key
    pub async fn get_session_key(&self, token: &str) -> Result<String> {
        let (_, key) = self.get_session(token).await?;
        Ok(key)
    }

    /// Scrobble a track
    pub async fn scrobble(&self, track: &Track, timestamp: i64, session_key: &str) -> Result<()> {
        if !self.enabled {
            return Err(anyhow!("Last.fm plugin is disabled"));
        }

        let artist = track.artist();
        let mut params = BTreeMap::new();
        params.insert("method", "track.scrobble".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("sk", session_key.to_string());
        params.insert("artist", artist);
        params.insert("track", track.title.clone());
        params.insert("timestamp", timestamp.to_string());
        params.insert("album", track.album.clone());

        if track.duration > 0 {
            params.insert("duration", track.duration.to_string());
        }
        if track.track > 0 {
            params.insert("trackNumber", track.track.to_string());
        }

        let sig = self.generate_signature(&params);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let resp = self
            .client
            .post(LASTFM_API_URL)
            .form(&params)
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;

        if let Some(error) = json.get("error") {
            let msg = json
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow!("Last.fm error {}: {}", error, msg));
        }

        Ok(())
    }

    /// Update now playing
    pub async fn update_now_playing(&self, track: &Track, session_key: &str) -> Result<()> {
        if !self.enabled {
            return Err(anyhow!("Last.fm plugin is disabled"));
        }

        let artist = track.artist();
        let mut params = BTreeMap::new();
        params.insert("method", "track.updateNowPlaying".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("sk", session_key.to_string());
        params.insert("artist", artist);
        params.insert("track", track.title.clone());
        params.insert("album", track.album.clone());

        if track.duration > 0 {
            params.insert("duration", track.duration.to_string());
        }

        let sig = self.generate_signature(&params);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let resp = self
            .client
            .post(LASTFM_API_URL)
            .form(&params)
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;

        if let Some(error) = json.get("error") {
            let msg = json
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow!("Last.fm error {}: {}", error, msg));
        }

        Ok(())
    }

    /// Check if track should be scrobbled
    /// Per Last.fm rules: duration > 30s and played >= min(duration/2, 240s)
    pub fn should_scrobble(track_duration: i32, play_duration: i32) -> bool {
        track_duration > 30 && play_duration >= std::cmp::min(track_duration / 2, 240)
    }
}

impl Default for LastFmPlugin {
    fn default() -> Self {
        Self::new()
    }
}
