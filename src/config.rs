//! Runtime configuration resolved from environment variables.
//!
//! Mirrors the Python `config.py`. All values are optional and most can be
//! overridden per tool call; they exist so the server can be wired up once (in
//! the MCP client config) without repeating settings on every request.

use std::time::Duration;

use anyhow::{bail, Context, Result};

/// Default ssh options: fail fast instead of hanging on a prompt. A stdio MCP
/// server has no TTY, so an interactive prompt would block the call forever.
pub const DEFAULT_SSH_OPTS: &[&str] = &[
    "-o",
    "BatchMode=yes",
    "-o",
    "StrictHostKeyChecking=accept-new",
];
pub const DEFAULT_YTDLP_TIMEOUT_SECS: u64 = 30 * 60;
pub const DEFAULT_TRANSFER_TIMEOUT_SECS: u64 = 10 * 60;

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
    pub history_path: Option<String>,
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
    /// Optional lowercase SHA-256 pins for auto-downloaded executable files.
    pub ytdlp_sha256: Option<String>,
    pub ffmpeg_sha256: Option<String>,
    /// Timeout budgets for external command phases.
    pub ytdlp_timeout_secs: u64,
    pub transfer_timeout_secs: u64,
}

impl Config {
    pub fn from_env_result() -> Result<Self> {
        let ssh_opts = match non_empty("YTDLP_SSH_OPTS") {
            Some(s) => parse_ssh_opts_result(&s).context("parse YTDLP_SSH_OPTS as shell words")?,
            None => Vec::new(),
        };

        Ok(Self {
            remote: non_empty("YTDLP_REMOTE"),
            dest_path: non_empty("YTDLP_REMOTE_PATH"),
            video_dest_path: non_empty("YTDLP_VIDEO_REMOTE_PATH"),
            staging_dir: non_empty("YTDLP_STAGING_DIR"),
            audio_format: non_empty("YTDLP_AUDIO_FORMAT").unwrap_or_else(|| "mp3".into()),
            ssh_opts,
            archive_dir: non_empty("YTDLP_ARCHIVE_DIR"),
            history_path: non_empty("YTDLP_HISTORY_PATH"),
            auto_update: as_bool("YTDLP_AUTO_UPDATE", true),
            max_age_days: as_int("YTDLP_MAX_AGE_DAYS", 14),
            update_pre: as_bool("YTDLP_UPDATE_PRE", false),
            ytdlp_path: non_empty("YTDLP_PATH"),
            ffmpeg_path: non_empty("FFMPEG_PATH"),
            extractor_args: non_empty("YTDLP_EXTRACTOR_ARGS"),
            ytdlp_sha256: sha256_pin_from_env("YTDLP_SHA256")?,
            ffmpeg_sha256: sha256_pin_from_env("FFMPEG_SHA256")?,
            ytdlp_timeout_secs: as_positive_u64("YTDLP_TIMEOUT_SECS", DEFAULT_YTDLP_TIMEOUT_SECS)?,
            transfer_timeout_secs: as_positive_u64(
                "YTDLP_TRANSFER_TIMEOUT_SECS",
                DEFAULT_TRANSFER_TIMEOUT_SECS,
            )?,
        })
    }

    #[allow(dead_code)]
    pub fn from_env() -> Self {
        Self::from_env_result().expect("invalid ytdl-mcp environment configuration")
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

    pub fn ytdlp_timeout(&self) -> Duration {
        Duration::from_secs(self.ytdlp_timeout_secs)
    }

    pub fn transfer_timeout(&self) -> Duration {
        Duration::from_secs(self.transfer_timeout_secs)
    }
}

pub(crate) fn normalize_sha256_pin(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(trimmed.to_ascii_lowercase())
    } else {
        None
    }
}

#[cfg(test)]
fn parse_ssh_opts(s: &str) -> Vec<String> {
    parse_ssh_opts_result(s).expect("invalid YTDLP_SSH_OPTS")
}

fn parse_ssh_opts_result(s: &str) -> Result<Vec<String>> {
    Ok(shell_words::split(s)?)
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

fn sha256_pin_from_env(key: &str) -> Result<Option<String>> {
    let Some(value) = non_empty(key) else {
        return Ok(None);
    };
    match normalize_sha256_pin(&value) {
        Some(pin) => Ok(Some(pin)),
        None => bail!("{key} must be exactly 64 lowercase or uppercase hex characters"),
    }
}

fn as_positive_u64(key: &str, default: u64) -> Result<u64> {
    let Ok(value) = std::env::var(key) else {
        return Ok(default);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    let parsed: u64 = trimmed
        .parse()
        .with_context(|| format!("{key} must be a positive integer"))?;
    if parsed == 0 {
        bail!("{key} must be greater than zero");
    }
    Ok(parsed)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
