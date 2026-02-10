//! Lyrics fetching and parsing

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Lyrics line with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsLine {
    pub time: Option<f64>, // Time in seconds
    pub text: String,
}

/// Full lyrics data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricsLine>,
    pub is_synced: bool,
    pub source: Option<String>,
    pub copyright: Option<String>,
}

/// Lyrics library
pub struct LyricsLib;

impl LyricsLib {
    /// Parse LRC format lyrics
    pub fn parse_lrc(content: &str) -> Lyrics {
        let mut lines = Vec::new();
        let mut is_synced = false;

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            // Try to parse timestamped line: [mm:ss.xx]text
            if let Some(parsed) = Self::parse_lrc_line(line) {
                is_synced = true;
                lines.push(parsed);
            } else if !line.starts_with('[') {
                // Plain text line
                lines.push(LyricsLine {
                    time: None,
                    text: line.to_string(),
                });
            }
        }

        // Sort by time if synced
        if is_synced {
            lines.sort_by(|a, b| {
                a.time
                    .partial_cmp(&b.time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        Lyrics {
            lines,
            is_synced,
            source: None,
            copyright: None,
        }
    }

    /// Parse single LRC line
    fn parse_lrc_line(line: &str) -> Option<LyricsLine> {
        // Match [mm:ss.xx] or [mm:ss]
        let re = regex::Regex::new(r"^\[(\d{1,2}):(\d{2})(?:\.(\d{2,3}))?\](.*)$").ok()?;

        let caps = re.captures(line)?;

        let minutes: f64 = caps.get(1)?.as_str().parse().ok()?;
        let seconds: f64 = caps.get(2)?.as_str().parse().ok()?;
        let milliseconds: f64 = caps
            .get(3)
            .map(|m| {
                m.as_str().parse::<f64>().unwrap_or(0.0)
                    / if m.as_str().len() == 2 { 100.0 } else { 1000.0 }
            })
            .unwrap_or(0.0);

        let time = minutes * 60.0 + seconds + milliseconds;
        let text = caps
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        Some(LyricsLine {
            time: Some(time),
            text,
        })
    }

    /// Convert lyrics to LRC format
    pub fn to_lrc(lyrics: &Lyrics) -> String {
        let mut output = String::new();

        for line in &lyrics.lines {
            if let Some(time) = line.time {
                let minutes = (time / 60.0).floor() as i32;
                let seconds = (time % 60.0).floor() as i32;
                let centiseconds = ((time * 100.0) % 100.0) as i32;

                output.push_str(&format!(
                    "[{:02}:{:02}.{:02}]{}\n",
                    minutes, seconds, centiseconds, line.text
                ));
            } else {
                output.push_str(&line.text);
                output.push('\n');
            }
        }

        output
    }

    /// Parse plain text lyrics
    pub fn parse_plain(content: &str) -> Lyrics {
        let lines: Vec<LyricsLine> = content
            .lines()
            .map(|line| LyricsLine {
                time: None,
                text: line.to_string(),
            })
            .collect();

        Lyrics {
            lines,
            is_synced: false,
            source: None,
            copyright: None,
        }
    }

    /// Get lyrics line at time
    pub fn get_line_at_time(lyrics: &Lyrics, time: f64) -> Option<&LyricsLine> {
        if !lyrics.is_synced {
            return None;
        }

        let mut current_line = None;

        for line in &lyrics.lines {
            if let Some(line_time) = line.time {
                if line_time <= time {
                    current_line = Some(line);
                } else {
                    break;
                }
            }
        }

        current_line
    }

    /// Search for lyrics from embedded metadata
    pub fn from_embedded(track_path: &std::path::Path) -> Option<Lyrics> {
        use lofty::{ItemKey, Probe, TaggedFileExt};

        let tagged_file = Probe::open(track_path).ok()?.read().ok()?;

        let tag = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag())?;

        // Try to find lyrics in common tag fields
        let lyrics_text = tag
            .get_string(&ItemKey::Lyrics)
            .or_else(|| tag.get_string(&ItemKey::Unknown("USLT".to_string())))
            .or_else(|| tag.get_string(&ItemKey::Unknown("SYLT".to_string())));

        lyrics_text.map(|text| Self::parse_lrc(text))
    }

    /// Check if text looks like LRC format
    pub fn is_lrc_format(content: &str) -> bool {
        content.lines().take(10).any(|line| {
            let trimmed = line.trim();
            regex::Regex::new(r"^\[\d{1,2}:\d{2}")
                .ok()
                .map(|re| re.is_match(trimmed))
                .unwrap_or(false)
        })
    }

    /// Fetch lyrics from external source (stub for now)
    pub async fn fetch(
        title: &str,
        artist: &str,
        album: Option<&str>,
        duration: u64,
    ) -> Result<FetchedLyrics> {
        // TODO: Implement actual lyrics fetching from external API
        Err(anyhow::anyhow!("Lyrics not found"))
    }
}

/// Fetched lyrics result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedLyrics {
    pub lyrics: String,
    pub synced: bool,
    pub source: String,
}
