//! Audio transcoding utilities using ffmpeg-sidecar

use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::core::ffmpeg;

/// Audio format/codec
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Flac,
    Ogg,
    Opus,
    Aac,
    Wav,
}

impl AudioFormat {
    /// get file extension
    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Flac => "flac",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "m4a",
            AudioFormat::Wav => "wav",
        }
    }

    /// get mime type
    pub fn mime_type(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Flac => "audio/flac",
            AudioFormat::Ogg => "audio/ogg",
            AudioFormat::Opus => "audio/opus",
            AudioFormat::Aac => "audio/mp4",
            AudioFormat::Wav => "audio/wav",
        }
    }

    /// get ffmpeg codec name
    pub fn ffmpeg_codec(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "libmp3lame",
            AudioFormat::Flac => "flac",
            AudioFormat::Ogg => "libvorbis",
            AudioFormat::Opus => "libopus",
            AudioFormat::Aac => "aac",
            AudioFormat::Wav => "pcm_s16le",
        }
    }

    /// get ffmpeg format name for pipe output
    pub fn ffmpeg_format(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Flac => "flac",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "adts",
            AudioFormat::Wav => "wav",
        }
    }

    /// parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mp3" => Some(AudioFormat::Mp3),
            "flac" => Some(AudioFormat::Flac),
            "ogg" | "vorbis" => Some(AudioFormat::Ogg),
            "opus" => Some(AudioFormat::Opus),
            "aac" | "m4a" => Some(AudioFormat::Aac),
            "wav" | "wave" => Some(AudioFormat::Wav),
            _ => None,
        }
    }
}

/// Audio quality preset
#[derive(Debug, Clone, Copy)]
pub enum Quality {
    Low,    // 128 kbps
    Medium, // 192 kbps
    High,   // 256 kbps
    Best,   // 320 kbps or lossless
}

impl Quality {
    /// get bitrate in kbps
    pub fn bitrate(&self) -> u32 {
        match self {
            Quality::Low => 128,
            Quality::Medium => 192,
            Quality::High => 256,
            Quality::Best => 320,
        }
    }
}

/// Audio transcoder using bundled ffmpeg
pub struct Transcoder;

impl Transcoder {
    /// check if ffmpeg is available (bundled or system)
    pub fn is_ffmpeg_available() -> bool {
        ffmpeg::is_ffmpeg_available()
    }

    /// ensure ffmpeg is available, downloading if necessary
    pub fn ensure_ffmpeg() -> Result<()> {
        ffmpeg::ensure_ffmpeg()
    }

    /// transcode audio file
    pub fn transcode(
        input: &Path,
        output: &Path,
        format: AudioFormat,
        quality: Quality,
    ) -> Result<()> {
        if !Self::is_ffmpeg_available() {
            Self::ensure_ffmpeg()?;
        }

        let ffmpeg_path = ffmpeg::get_ffmpeg_path();
        let mut cmd = Command::new(&ffmpeg_path);

        cmd.args([
            "-i",
            input.to_str().unwrap(),
            "-y", // overwrite output
        ]);

        // add codec-specific options
        match format {
            AudioFormat::Mp3 => {
                cmd.args([
                    "-c:a",
                    "libmp3lame",
                    "-b:a",
                    &format!("{}k", quality.bitrate()),
                ]);
            }
            AudioFormat::Flac => {
                cmd.args(["-c:a", "flac", "-compression_level", "8"]);
            }
            AudioFormat::Ogg => {
                cmd.args(["-c:a", "libvorbis", "-q:a", &Self::vorbis_quality(quality)]);
            }
            AudioFormat::Opus => {
                cmd.args([
                    "-c:a",
                    "libopus",
                    "-b:a",
                    &format!("{}k", quality.bitrate()),
                ]);
            }
            AudioFormat::Aac => {
                cmd.args(["-c:a", "aac", "-b:a", &format!("{}k", quality.bitrate())]);
            }
            AudioFormat::Wav => {
                cmd.args(["-c:a", "pcm_s16le"]);
            }
        }

        cmd.arg(output.to_str().unwrap());

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("ffmpeg error: {}", stderr));
        }

        Ok(())
    }

    /// get vorbis quality setting (0-10)
    fn vorbis_quality(quality: Quality) -> String {
        match quality {
            Quality::Low => "3",
            Quality::Medium => "5",
            Quality::High => "7",
            Quality::Best => "9",
        }
        .to_string()
    }

    /// transcode to bytes (for streaming)
    pub fn transcode_to_bytes(
        input: &Path,
        format: AudioFormat,
        quality: Quality,
    ) -> Result<Vec<u8>> {
        if !Self::is_ffmpeg_available() {
            Self::ensure_ffmpeg()?;
        }

        ffmpeg::transcode_to_bytes(
            input,
            format.ffmpeg_format(),
            format.ffmpeg_codec(),
            Some(quality.bitrate()),
        )
    }

    /// get audio stream command for http range requests
    pub fn create_stream_command(
        input: &Path,
        format: AudioFormat,
        quality: Quality,
        start_time: Option<f64>,
    ) -> Command {
        ffmpeg::create_transcode_command(
            input,
            format.ffmpeg_format(),
            format.ffmpeg_codec(),
            Some(quality.bitrate()),
            start_time,
        )
    }
}
