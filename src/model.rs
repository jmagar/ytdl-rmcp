//! Tool input models + enums (ports `models.py`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum DownloadMode {
    #[default]
    Audio,
    Video,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Best,
    #[default]
    Mp3,
    M4a,
    Opus,
    Flac,
    Wav,
}

impl AudioFormat {
    /// Parse from a config string (YTDLP_AUDIO_FORMAT), defaulting to mp3.
    pub fn parse_or_default(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "best" => AudioFormat::Best,
            "m4a" => AudioFormat::M4a,
            "opus" => AudioFormat::Opus,
            "flac" => AudioFormat::Flac,
            "wav" => AudioFormat::Wav,
            _ => AudioFormat::Mp3,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            AudioFormat::Best => "best",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Opus => "opus",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
        }
    }
    /// Codecs where a quality knob does not apply.
    pub fn is_lossless_or_passthrough(self) -> bool {
        matches!(
            self,
            AudioFormat::Best | AudioFormat::Flac | AudioFormat::Wav
        )
    }
    /// Containers that can hold tags + embedded cover art (WAV cannot).
    pub fn is_taggable(self) -> bool {
        !matches!(self, AudioFormat::Wav)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum VideoContainer {
    #[default]
    Mp4,
    Mkv,
}

impl VideoContainer {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoContainer::Mp4 => "mp4",
            VideoContainer::Mkv => "mkv",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    #[default]
    Markdown,
    Json,
}

/// Accept either a single URL string or a list of URL strings.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Urls {
    One(String),
    Many(Vec<String>),
}

impl Urls {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Urls::One(s) => vec![s],
            Urls::Many(v) => v,
        }
    }
}

fn default_audio_quality() -> String {
    "0".into()
}

/// Input for `youtube_download`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DownloadInput {
    /// One or more video URLs supported by yt-dlp. A single URL string is accepted.
    pub urls: Urls,
    /// 'audio' (default), 'video', or 'both'.
    #[serde(default)]
    pub mode: DownloadMode,
    /// Audio codec when mode includes audio. Falls back to YTDLP_AUDIO_FORMAT.
    #[serde(default)]
    pub audio_format: Option<AudioFormat>,
    /// yt-dlp audio quality for lossy codecs: '0' (best VBR) to '9', or a bitrate like '192K'.
    #[serde(default = "default_audio_quality")]
    pub audio_quality: String,
    /// Cap video resolution by height, e.g. 1080 or 2160. None = best available.
    #[serde(default)]
    pub max_height: Option<u32>,
    /// Output container when downloading video.
    #[serde(default)]
    pub container: VideoContainer,
    /// SSH remote (alias or user@host). Falls back to YTDLP_REMOTE.
    #[serde(default)]
    pub remote: Option<String>,
    /// Absolute remote dir for audio. Falls back to YTDLP_REMOTE_PATH.
    #[serde(default)]
    pub dest_path: Option<String>,
    /// Absolute remote dir for video. Falls back to YTDLP_VIDEO_REMOTE_PATH, then dest_path.
    #[serde(default)]
    pub video_dest_path: Option<String>,
    /// Keep the local staging copy after a successful transfer.
    #[serde(default)]
    pub keep_local: bool,
    /// Record downloaded IDs and skip them on future calls (per mode).
    #[serde(default)]
    pub use_archive: bool,
    /// 'markdown' (human-readable) or 'json' (machine-readable).
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Input for `youtube_probe`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ProbeInput {
    /// One or more video URLs. A single URL string is accepted.
    pub urls: Urls,
    /// 'markdown' (human-readable) or 'json' (machine-readable).
    #[serde(default)]
    pub response_format: ResponseFormat,
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
