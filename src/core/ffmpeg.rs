//! ffmpeg and ffprobe utilities using bundled binaries via ffmpeg-sidecar
//!
//! this module provides wrappers around ffmpeg-sidecar for audio transcoding
//! and metadata extraction without requiring system ffmpeg installation

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};

// re-export commonly used items from ffmpeg-sidecar
pub use ffmpeg_sidecar::command::FfmpegCommand;
pub use ffmpeg_sidecar::download::auto_download;
pub use ffmpeg_sidecar::ffprobe::{ffprobe_path, ffprobe_is_installed};

/// metadata extracted from audio file via ffprobe
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioMetadata {
    pub duration: f64,
    pub bitrate: i32,
    pub sample_rate: i32,
    pub channels: i32,
    pub codec: String,
    pub format: String,
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub track: Option<i32>,
    pub disc: Option<i32>,
    pub date: Option<String>,
    pub genre: Option<String>,
    pub copyright: Option<String>,
}

/// ffprobe json output format structure
#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    streams: Option<Vec<FfprobeStream>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
    format_name: Option<String>,
    tags: Option<FfprobeTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    sample_rate: Option<String>,
    channels: Option<i32>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeTags {
    title: Option<String>,
    album: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    #[serde(alias = "ALBUM_ARTIST")]
    album_artist_upper: Option<String>,
    track: Option<String>,
    disc: Option<String>,
    date: Option<String>,
    genre: Option<String>,
    copyright: Option<String>,
    #[serde(alias = "TITLE")]
    title_upper: Option<String>,
    #[serde(alias = "ALBUM")]
    album_upper: Option<String>,
    #[serde(alias = "ARTIST")]
    artist_upper: Option<String>,
    #[serde(alias = "TRACK")]
    track_upper: Option<String>,
    #[serde(alias = "DISC")]
    disc_upper: Option<String>,
    #[serde(alias = "DATE")]
    date_upper: Option<String>,
    #[serde(alias = "GENRE")]
    genre_upper: Option<String>,
    #[serde(alias = "COPYRIGHT")]
    copyright_upper: Option<String>,
}

/// ensures ffmpeg and ffprobe are available, downloading if necessary
pub fn ensure_ffmpeg() -> Result<()> {
    if !ffmpeg_sidecar::command::ffmpeg_is_installed() {
        tracing::info!("ffmpeg not found, downloading...");
        auto_download().context("failed to download ffmpeg")?;
        tracing::info!("ffmpeg downloaded successfully");
    }
    Ok(())
}

/// checks if ffmpeg is available (either system or sidecar)
pub fn is_ffmpeg_available() -> bool {
    ffmpeg_sidecar::command::ffmpeg_is_installed()
}

/// checks if ffprobe is available (either system or sidecar)
pub fn is_ffprobe_available() -> bool {
    ffprobe_is_installed()
}

/// gets the path to the ffmpeg binary
pub fn get_ffmpeg_path() -> std::path::PathBuf {
    ffmpeg_sidecar::paths::ffmpeg_path()
}

/// gets the path to the ffprobe binary
pub fn get_ffprobe_path() -> std::path::PathBuf {
    ffprobe_path()
}

/// extracts metadata from an audio file using ffprobe
pub fn probe_metadata(path: &Path) -> Result<AudioMetadata> {
    let ffprobe = get_ffprobe_path();
    
    let output = Command::new(&ffprobe)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to execute ffprobe")?;

    if !output.status.success() {
        anyhow::bail!("ffprobe failed with status: {}", output.status);
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let probe: FfprobeOutput = serde_json::from_str(&json_str)
        .context("failed to parse ffprobe json output")?;

    let mut metadata = AudioMetadata::default();

    // extract format info
    if let Some(format) = &probe.format {
        if let Some(duration) = &format.duration {
            metadata.duration = duration.parse().unwrap_or(0.0);
        }
        if let Some(bitrate) = &format.bit_rate {
            metadata.bitrate = bitrate.parse::<i64>().unwrap_or(0) as i32 / 1000;
        }
        if let Some(format_name) = &format.format_name {
            metadata.format = format_name.clone();
        }

        // extract tags
        if let Some(tags) = &format.tags {
            metadata.title = tags.title.clone().or_else(|| tags.title_upper.clone());
            metadata.album = tags.album.clone().or_else(|| tags.album_upper.clone());
            metadata.artist = tags.artist.clone().or_else(|| tags.artist_upper.clone());
            metadata.album_artist = tags.album_artist.clone()
                .or_else(|| tags.album_artist_upper.clone());
            metadata.genre = tags.genre.clone().or_else(|| tags.genre_upper.clone());
            metadata.copyright = tags.copyright.clone().or_else(|| tags.copyright_upper.clone());
            metadata.date = tags.date.clone().or_else(|| tags.date_upper.clone());
            
            // parse track number (might be "1/12" format)
            let track_str = tags.track.clone().or_else(|| tags.track_upper.clone());
            if let Some(t) = track_str {
                metadata.track = t.split('/').next()
                    .and_then(|s| s.trim().parse().ok());
            }
            
            // parse disc number (might be "1/2" format)
            let disc_str = tags.disc.clone().or_else(|| tags.disc_upper.clone());
            if let Some(d) = disc_str {
                metadata.disc = d.split('/').next()
                    .and_then(|s| s.trim().parse().ok());
            }
        }
    }

    // extract stream info (first audio stream)
    if let Some(streams) = &probe.streams {
        for stream in streams {
            if stream.codec_type.as_deref() == Some("audio") {
                if let Some(codec) = &stream.codec_name {
                    metadata.codec = codec.clone();
                }
                if let Some(sample_rate) = &stream.sample_rate {
                    metadata.sample_rate = sample_rate.parse().unwrap_or(0);
                }
                if let Some(channels) = stream.channels {
                    metadata.channels = channels;
                }
                // stream bitrate might be more accurate than format bitrate
                if metadata.bitrate == 0 {
                    if let Some(bitrate) = &stream.bit_rate {
                        metadata.bitrate = bitrate.parse::<i64>().unwrap_or(0) as i32 / 1000;
                    }
                }
                break;
            }
        }
    }

    Ok(metadata)
}

/// gets just the duration of an audio file in seconds
pub fn get_duration(path: &Path) -> Result<f64> {
    let ffprobe = get_ffprobe_path();
    
    let output = Command::new(&ffprobe)
        .args([
            "-v", "quiet",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to execute ffprobe")?;

    if !output.status.success() {
        anyhow::bail!("ffprobe failed");
    }

    let duration_str = String::from_utf8_lossy(&output.stdout);
    duration_str.trim()
        .parse()
        .context("failed to parse duration")
}

/// creates an ffmpeg command builder configured with the sidecar binary path
pub fn ffmpeg_command() -> FfmpegCommand {
    FfmpegCommand::new()
}

/// transcodes audio using ffmpeg to the specified format
pub fn transcode_audio(
    input: &Path,
    output: &Path,
    codec: &str,
    bitrate_kbps: Option<u32>,
) -> Result<()> {
    let ffmpeg = get_ffmpeg_path();
    
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-i"])
        .arg(input)
        .args(["-y"]); // overwrite output

    // set audio codec
    cmd.args(["-c:a", codec]);
    
    // set bitrate if specified
    if let Some(br) = bitrate_kbps {
        cmd.args(["-b:a", &format!("{}k", br)]);
    }
    
    cmd.arg(output);

    let output_result = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to execute ffmpeg")?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        anyhow::bail!("ffmpeg transcode failed: {}", stderr);
    }

    Ok(())
}

/// transcodes audio to bytes (for streaming) using pipe output
pub fn transcode_to_bytes(
    input: &Path,
    format: &str,
    codec: &str,
    bitrate_kbps: Option<u32>,
) -> Result<Vec<u8>> {
    let ffmpeg = get_ffmpeg_path();
    
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-i"])
        .arg(input)
        .args(["-f", format])
        .args(["-c:a", codec]);
    
    if let Some(br) = bitrate_kbps {
        cmd.args(["-b:a", &format!("{}k", br)]);
    }
    
    cmd.arg("pipe:1"); // output to stdout

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to execute ffmpeg")?;

    if !output.status.success() {
        anyhow::bail!("ffmpeg transcode failed");
    }

    Ok(output.stdout)
}

/// creates an ffmpeg transcode command for streaming (returns the Command for manual control)
pub fn create_transcode_command(
    input: &Path,
    format: &str,
    codec: &str,
    bitrate_kbps: Option<u32>,
    start_time: Option<f64>,
) -> Command {
    let ffmpeg = get_ffmpeg_path();
    let mut cmd = Command::new(&ffmpeg);

    if let Some(start) = start_time {
        cmd.args(["-ss", &format!("{}", start)]);
    }

    cmd.args(["-i"])
        .arg(input)
        .args(["-f", format])
        .args(["-c:a", codec]);
    
    if let Some(br) = bitrate_kbps {
        cmd.args(["-b:a", &format!("{}k", br)]);
    }
    
    cmd.arg("pipe:1");
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffmpeg_available() {
        // this just tests that the check doesn't panic
        let _ = is_ffmpeg_available();
    }

    #[test]
    fn test_ffprobe_available() {
        let _ = is_ffprobe_available();
    }
}
