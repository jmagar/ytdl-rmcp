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

impl DownloadMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DownloadMode::Audio => "audio",
            DownloadMode::Video => "video",
            DownloadMode::Both => "both",
        }
    }
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

/// Accept either a single string or a list of strings. Shared shape behind both
/// [`Urls`] and [`Paths`] (they were byte-identical enums; BP-M5).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

/// Accept either a single URL string or a list of URL strings.
///
/// A distinct newtype (not a `OneOrMany` alias) so the only way to extract the
/// values is [`Urls::into_validated_vec`], which enforces the http(s) backstop.
/// There is deliberately no public unvalidated extractor on `Urls`: a URL caller
/// cannot reach the yt-dlp subprocess without passing validation, and that is a
/// compile-time guarantee rather than a naming convention. `#[serde(transparent)]`
/// keeps the wire shape (string-or-array) and the generated JSON schema identical
/// to the bare `OneOrMany` — schemars reads `transparent` from the serde attr and
/// delegates to `OneOrMany`'s `string | array` schema.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Urls(OneOrMany);

/// Accept either a single local file path string or a list of paths.
///
/// Local paths are not URLs, so they get a plain [`Paths::into_vec`] with no
/// scheme check — and crucially they do NOT expose `into_validated_vec`, so the
/// URL backstop can never be wrongly applied to (or skipped for) local files.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Paths(OneOrMany);

impl Urls {
    /// Validate that every entry is a well-formed `http`/`https` URL before it
    /// reaches the yt-dlp subprocess, returning the trimmed values. This is the
    /// ONLY extractor on `Urls`: there is no unvalidated path, so the backstop is
    /// not opt-in. Defense-in-depth for the `--` end-of-options guard at the
    /// yt-dlp call sites (F2): rejects values such as `--exec=...`, `-o /path`, or
    /// non-http(s) schemes that yt-dlp would otherwise interpret as flags.
    ///
    /// The download and probe paths in `service.rs` already call this; the local
    /// `paths` path (identify) uses [`Paths::into_vec`] instead, as those are
    /// files, not URLs.
    pub fn into_validated_vec(self) -> anyhow::Result<Vec<String>> {
        self.0
            .into_vec()
            .into_iter()
            .map(|url| validate_http_url(&url))
            .collect()
    }

    /// Construct from a single URL. Named to read like the former `Urls::One`
    /// enum variant so existing test construction sites keep working after the
    /// newtype refactor. Construction does not validate; [`Urls::into_validated_vec`]
    /// is the single validation gate.
    #[cfg(test)]
    #[allow(non_snake_case)]
    pub fn One(url: String) -> Self {
        Urls(OneOrMany::One(url))
    }
}

impl Paths {
    /// Extract the local file paths. No scheme validation: these are local files,
    /// not URLs.
    pub fn into_vec(self) -> Vec<String> {
        self.0.into_vec()
    }

    /// Construct from a single path. Named to read like the former `Paths::One`
    /// enum variant so existing test construction sites keep working after the
    /// newtype refactor.
    #[cfg(test)]
    #[allow(non_snake_case)]
    pub fn One(path: String) -> Self {
        Paths(OneOrMany::One(path))
    }
}

/// Reject anything that is not an `http`/`https` URL with a clear error, and on
/// success return the trimmed value so a leading-space URL never reaches yt-dlp.
///
/// Embedded control characters (newline, CR, NUL, etc.) are rejected up front
/// (SEC-F2): `trim()` only strips surrounding whitespace, so without this an
/// interior `\n`/`\r` would survive into the subprocess argv, the JSONL history
/// ledger, and reflected error messages — enabling log/ledger injection.
fn validate_http_url(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.chars().any(char::is_control) {
        anyhow::bail!("invalid URL {value:?}: contains control characters");
    }
    if crate::util::is_http_url(trimmed) {
        // Require a non-empty host component after the scheme separator.
        let rest = trimmed
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        if rest
            .chars()
            .next()
            .is_some_and(|c| !matches!(c, '/' | '?' | '#'))
        {
            return Ok(trimmed.to_string());
        }
    }
    anyhow::bail!(
        "invalid URL {value:?}: only http:// and https:// URLs are accepted \
         (a leading '-' or other scheme is rejected to prevent it being parsed \
         as a yt-dlp option)"
    )
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
    /// Destination target. Use `/path` for local, `host:/path` for SSH, or `remote:path` for rclone.
    #[serde(default)]
    pub target_path: Option<String>,
    /// Destination target for video. Falls back to YTDLP_VIDEO_TARGET_PATH, then target_path.
    #[serde(default)]
    pub video_target_path: Option<String>,
    /// Deprecated: SSH remote alias or user@host. Use `target_path` instead.
    #[serde(default)]
    pub remote: Option<String>,
    /// Deprecated: SSH destination path. Use `target_path` instead.
    #[serde(default)]
    pub dest_path: Option<String>,
    /// Deprecated: SSH video destination path. Use `video_target_path` instead.
    #[serde(default)]
    pub video_dest_path: Option<String>,
    /// Keep the local staging copy after a successful transfer.
    #[serde(default)]
    pub keep_local: bool,
    /// Record downloaded IDs and skip them on future calls (per mode).
    #[serde(default)]
    pub use_archive: bool,
    /// Plex playlist title or ID to add downloaded audio tracks to. Falls back to YTDLP_PLEX_PLAYLIST.
    #[serde(default)]
    pub plex_playlist: Option<String>,
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

/// Input for `youtube_identify`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct IdentifyInput {
    /// One or more local audio file paths to fingerprint and identify.
    pub paths: Paths,
    /// Write high-confidence MusicBrainz retag previews back to the files.
    #[serde(default)]
    pub write_tags: bool,
    /// 'markdown' (human-readable) or 'json' (machine-readable).
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_search_limit() -> u32 {
    10
}

/// Upper bound on `youtube_search` results. Single source of truth shared with
/// `downloader::search_spec` (L5).
pub const MAX_SEARCH_LIMIT: u32 = 25;

/// Input for `youtube_search` and `youtube_search_ui`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchInput {
    /// YouTube search terms. This is passed to yt-dlp as `ytsearchN:<query>`.
    pub query: String,
    /// Number of YouTube results to return. Clamped to 1..=25.
    #[serde(default = "default_search_limit")]
    pub limit: u32,
    /// 'markdown' (human-readable) or 'json' (machine-readable).
    #[serde(default)]
    pub response_format: ResponseFormat,
}

impl SearchInput {
    pub fn effective_limit(&self) -> u32 {
        self.limit.clamp(1, MAX_SEARCH_LIMIT)
    }
}

fn default_stats_limit() -> usize {
    10
}

/// Input for `youtube_stats`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StatsInput {
    /// Number of recent history entries to include. Clamped to 0..=100.
    #[serde(default = "default_stats_limit")]
    pub limit: usize,
    /// 'markdown' (human-readable) or 'json' (machine-readable).
    #[serde(default)]
    pub response_format: ResponseFormat,
}

impl StatsInput {
    pub fn effective_limit(&self) -> usize {
        self.limit.min(100)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchResultItem {
    pub title: String,
    pub url: String,
    pub video_id: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub view_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SearchPayload {
    pub query: String,
    pub limit: u32,
    pub results: Vec<SearchResultItem>,
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
