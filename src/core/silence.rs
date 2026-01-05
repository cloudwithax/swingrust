//! Silence detection in audio files using ffmpeg

use anyhow::Result;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::core::ffmpeg;

/// Silence detection result
#[derive(Debug, Clone)]
pub struct SilenceInfo {
    /// silence at start in seconds
    pub silence_start: f64,
    /// silence at end in seconds
    pub silence_end: f64,
    /// total track duration in seconds
    pub duration: f64,
}

/// Silence detection utilities
pub struct SilenceDetector;

impl SilenceDetector {
    /// default silence threshold in dB
    const DEFAULT_THRESHOLD_DB: f32 = -50.0;

    /// detect silence at start and end of audio file using ffmpeg
    pub fn detect(path: &Path) -> Result<SilenceInfo> {
        Self::detect_with_threshold(path, Self::DEFAULT_THRESHOLD_DB)
    }

    /// detect silence with custom threshold
    pub fn detect_with_threshold(path: &Path, threshold_db: f32) -> Result<SilenceInfo> {
        // ensure ffmpeg is available
        ffmpeg::ensure_ffmpeg()?;

        // get duration first
        let duration = ffmpeg::get_duration(path)?;

        // run silence detection filter
        let ffmpeg_path = ffmpeg::get_ffmpeg_path();
        let output = Command::new(&ffmpeg_path)
            .args([
                "-i",
            ])
            .arg(path)
            .args([
                "-af",
                &format!("silencedetect=noise={}dB:d=0.5", threshold_db),
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        // parse silence info from ffmpeg output
        let silence_start = Self::parse_silence_start(&stderr);
        let silence_end = Self::calculate_silence_end(&stderr, duration);

        Ok(SilenceInfo {
            silence_start,
            silence_end,
            duration,
        })
    }

    /// parse first silence_start from ffmpeg output
    fn parse_silence_start(output: &str) -> f64 {
        // format: [silencedetect @ ...] silence_start: X.XXX
        for line in output.lines() {
            if line.contains("silence_start:") {
                if let Some(value) = line.split("silence_start:").nth(1) {
                    if let Ok(v) = value.trim().split_whitespace().next()
                        .unwrap_or("")
                        .parse::<f64>() 
                    {
                        // only return if silence starts near the beginning
                        if v < 1.0 {
                            return v;
                        }
                    }
                }
            }
        }
        0.0
    }

    /// calculate silence at end from last silence_end detection
    fn calculate_silence_end(output: &str, duration: f64) -> f64 {
        // format: [silencedetect @ ...] silence_end: X.XXX | silence_duration: Y.YYY
        let mut last_silence_end = 0.0;
        let mut last_silence_duration = 0.0;

        for line in output.lines() {
            if line.contains("silence_end:") {
                if let Some(end_part) = line.split("silence_end:").nth(1) {
                    if let Some(end_str) = end_part.split_whitespace().next() {
                        if let Ok(end) = end_str.parse::<f64>() {
                            last_silence_end = end;
                        }
                    }
                }
                if let Some(dur_part) = line.split("silence_duration:").nth(1) {
                    if let Some(dur_str) = dur_part.split_whitespace().next() {
                        if let Ok(dur) = dur_str.parse::<f64>() {
                            last_silence_duration = dur;
                        }
                    }
                }
            }
        }

        // if the last silence ends at or near the track end, return its duration
        if last_silence_end > 0.0 && (duration - last_silence_end).abs() < 0.5 {
            return last_silence_duration;
        }

        0.0
    }

    /// get recommended playback boundaries
    pub fn get_playback_bounds(info: &SilenceInfo) -> (f64, f64) {
        (info.silence_start, info.duration - info.silence_end)
    }

    /// format silence info as human readable string
    pub fn format_info(info: &SilenceInfo) -> String {
        format!(
            "duration: {:.2}s, silence start: {:.2}s, silence end: {:.2}s",
            info.duration, info.silence_start, info.silence_end
        )
    }
}
