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
pub const DEFAULT_PLEX_PLAYLIST: &str = "yt-dlp Downloads";

#[derive(Debug, Clone)]
pub struct Config {
    pub target_path: Option<String>,
    pub video_target_path: Option<String>,
    pub allow_local_targets: bool,
    pub staging_dir: Option<String>,
    pub audio_format: String,
    /// Extra ssh options appended after [`DEFAULT_SSH_OPTS`].
    pub ssh_opts: Vec<String>,
    pub archive_dir: Option<String>,
    pub history_path: Option<String>,
    pub plex_url: Option<String>,
    pub plex_token: Option<String>,
    pub plex_playlist: Option<String>,
    pub clean_metadata: bool,
    pub acoustid_client_key: Option<String>,
    pub fpcalc_path: Option<String>,
    pub musicbrainz_contact: Option<String>,
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

        let plex_url = non_empty("YTDLP_PLEX_URL");
        let plex_token = non_empty("YTDLP_PLEX_TOKEN");
        let plex_playlist = non_empty("YTDLP_PLEX_PLAYLIST").or_else(|| {
            if plex_url.is_some() && plex_token.is_some() {
                Some(DEFAULT_PLEX_PLAYLIST.into())
            } else {
                None
            }
        });

        let legacy_audio_target = legacy_target_path("YTDLP_REMOTE_PATH");
        let target_path = non_empty("YTDLP_TARGET_PATH").or(legacy_audio_target);
        let video_target_path = non_empty("YTDLP_VIDEO_TARGET_PATH")
            .or_else(|| legacy_target_path("YTDLP_VIDEO_REMOTE_PATH"));

        Ok(Self {
            target_path,
            video_target_path,
            allow_local_targets: as_bool("YTDLP_ALLOW_LOCAL_TARGETS", false),
            staging_dir: non_empty("YTDLP_STAGING_DIR"),
            audio_format: non_empty("YTDLP_AUDIO_FORMAT").unwrap_or_else(|| "mp3".into()),
            ssh_opts,
            archive_dir: non_empty("YTDLP_ARCHIVE_DIR"),
            history_path: non_empty("YTDLP_HISTORY_PATH"),
            plex_url,
            plex_token,
            plex_playlist,
            clean_metadata: as_bool("YTDLP_CLEAN_METADATA", true),
            acoustid_client_key: non_empty("YTDLP_ACOUSTID_CLIENT_KEY"),
            fpcalc_path: non_empty("FPCALC_PATH"),
            musicbrainz_contact: non_empty("YTDLP_MUSICBRAINZ_CONTACT"),
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

    /// Panicking convenience wrapper over [`from_env_result`](Self::from_env_result),
    /// gated to test builds only (`#[cfg(test)]`) so it can never be wired into
    /// production startup. Production code paths must use `from_env_result`.
    #[cfg(test)]
    pub fn from_env() -> Self {
        Self::from_env_result().expect("invalid ytdl-rmcp environment configuration")
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

fn legacy_target_path(path_env: &str) -> Option<String> {
    let remote = non_empty("YTDLP_REMOTE")?;
    let path = non_empty(path_env)?;
    Some(format!("ssh:{remote}:{path}"))
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
    Ok(strip_dangerous_ssh_opts(shell_words::split(s)?))
}

/// SSH `-o` option keys that let an operator turn a transfer into arbitrary
/// command execution (e.g. `-o ProxyCommand=...`, `-o PermitLocalCommand=yes`
/// `-o LocalCommand=...`). Compared case-insensitively against the key before
/// the `=`.
const DANGEROUS_SSH_OPT_KEYS: &[&str] = &["ProxyCommand", "LocalCommand", "PermitLocalCommand"];

/// Defense-in-depth filter for operator-supplied `YTDLP_SSH_OPTS`.
///
/// `YTDLP_SSH_OPTS` is operator-controlled under this project's trust model, so
/// this is hardening rather than a caller-reachable hole. Rather than
/// hard-rejecting (which would brick the whole config on a single footgun), we
/// **warn-and-strip**: any `-o`/`-oKEY=...` token whose key enables command
/// execution ([`DANGEROUS_SSH_OPT_KEYS`]) is dropped with a clear `tracing`
/// warning to stderr, while every other override (including unlisted `-o`
/// options like `ConnectTimeout`) passes through untouched. Handles both the
/// glued form (`-oProxyCommand=...`) and the split form (`-o`
/// `ProxyCommand=...`).
fn strip_dangerous_ssh_opts(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut iter = tokens.into_iter().peekable();
    while let Some(token) = iter.next() {
        if token == "-o" {
            // Split form: `-o` followed by `KEY=VALUE` in the next token.
            if let Some(value) = iter.peek() {
                if is_dangerous_ssh_opt_value(value) {
                    tracing::warn!(
                        option = %value,
                        "dropping dangerous ssh option from YTDLP_SSH_OPTS (enables command execution)"
                    );
                    iter.next(); // consume the value too
                    continue;
                }
            }
            out.push(token);
        } else if let Some(value) = token.strip_prefix("-o") {
            // Glued form: `-oKEY=VALUE`.
            if is_dangerous_ssh_opt_value(value) {
                tracing::warn!(
                    option = %token,
                    "dropping dangerous ssh option from YTDLP_SSH_OPTS (enables command execution)"
                );
                continue;
            }
            out.push(token);
        } else {
            out.push(token);
        }
    }
    out
}

/// True if `value` (the `KEY=...` portion of an ssh `-o` option) names a key in
/// [`DANGEROUS_SSH_OPT_KEYS`], compared case-insensitively.
fn is_dangerous_ssh_opt_value(value: &str) -> bool {
    let key = value.split('=').next().unwrap_or("").trim();
    DANGEROUS_SSH_OPT_KEYS
        .iter()
        .any(|dangerous| key.eq_ignore_ascii_case(dangerous))
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
