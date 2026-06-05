//! Runtime configuration resolved from environment variables.
//!
//! Mirrors the Python `config.py`. All values are optional and most can be
//! overridden per tool call; they exist so the server can be wired up once (in
//! the MCP client config) without repeating settings on every request.

/// Default ssh options: fail fast instead of hanging on a prompt. A stdio MCP
/// server has no TTY, so an interactive prompt would block the call forever.
pub const DEFAULT_SSH_OPTS: &[&str] = &[
    "-o",
    "BatchMode=yes",
    "-o",
    "StrictHostKeyChecking=accept-new",
];

#[derive(Debug, Clone)]
pub struct Config {
    pub remote: Option<String>,
    pub dest_path: Option<String>,
    pub video_dest_path: Option<String>,
    pub staging_dir: Option<String>,
    pub audio_format: String,
    /// Extra ssh options appended after [`DEFAULT_SSH_OPTS`].
    pub ssh_opts: Vec<String>,
    pub archive_dir: Option<String>,
    pub auto_update: bool,
    pub max_age_days: i64,
    pub update_pre: bool,
    /// Explicit yt-dlp / ffmpeg binary overrides (else resolved by bootstrap).
    pub ytdlp_path: Option<String>,
    pub ffmpeg_path: Option<String>,
    /// Passed to yt-dlp as `--extractor-args` (e.g.
    /// `youtube:player_client=android`) for sites/videos the default clients
    /// can't reach.
    pub extractor_args: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            remote: non_empty("YTDLP_REMOTE"),
            dest_path: non_empty("YTDLP_REMOTE_PATH"),
            video_dest_path: non_empty("YTDLP_VIDEO_REMOTE_PATH"),
            staging_dir: non_empty("YTDLP_STAGING_DIR"),
            audio_format: non_empty("YTDLP_AUDIO_FORMAT").unwrap_or_else(|| "mp3".into()),
            ssh_opts: non_empty("YTDLP_SSH_OPTS")
                .map(|s| s.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
            archive_dir: non_empty("YTDLP_ARCHIVE_DIR"),
            auto_update: as_bool("YTDLP_AUTO_UPDATE", true),
            max_age_days: as_int("YTDLP_MAX_AGE_DAYS", 14),
            update_pre: as_bool("YTDLP_UPDATE_PRE", false),
            ytdlp_path: non_empty("YTDLP_PATH"),
            ffmpeg_path: non_empty("FFMPEG_PATH"),
            extractor_args: non_empty("YTDLP_EXTRACTOR_ARGS"),
        }
    }

    /// Full ssh option list: forced defaults followed by any user extras.
    pub fn all_ssh_opts(&self) -> Vec<String> {
        DEFAULT_SSH_OPTS
            .iter()
            .copied()
            .chain(self.ssh_opts.iter().map(String::as_str))
            .map(str::to_string)
            .collect()
    }
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn as_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn as_int(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
