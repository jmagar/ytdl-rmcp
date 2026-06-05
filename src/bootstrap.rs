//! Resolve — and, when missing, auto-install — the external binaries the server
//! shells out to: yt-dlp and ffmpeg.
//!
//! Resolution order for each tool: explicit env override → PATH → per-user
//! cache dir → download into the cache dir. A lockfile serializes concurrent
//! first-run downloads.

mod ffmpeg;
mod http;
mod ytdlp;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use fs2::FileExt;

use crate::config::Config;

/// Resolved paths to the external tools used by a download, plus pass-through
/// yt-dlp options that apply to every invocation.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ytdlp: PathBuf,
    /// Directory containing ffmpeg (passed to yt-dlp via `--ffmpeg-location`),
    /// or None to let yt-dlp find ffmpeg itself.
    pub ffmpeg_dir: Option<PathBuf>,
    /// Value for yt-dlp's `--extractor-args`, if configured.
    pub extractor_args: Option<String>,
}

/// This app's per-user directories (cache/state/…). Single source of the
/// `tv/tootie/ytdl-mcp` identity.
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("tv", "tootie", "ytdl-mcp")
}

/// Per-user cache dir holding downloaded binaries (`<cache>/bin`).
pub fn cache_bin_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.cache_dir().join("bin"))
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-mcp/bin"))
}

/// Platform executable name: appends `.exe` on Windows.
pub(crate) fn exe_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// Shared prefix of every tool resolver: explicit override → PATH. Returns
/// `Some(path)` when found, `None` when the caller should fall through to its
/// own cache/download logic. Errors only if an override is set but missing.
pub(crate) fn resolve_override_or_path(
    override_path: Option<&str>,
    env_var: &str,
    bin_name: &str,
) -> Result<Option<PathBuf>> {
    if let Some(p) = override_path {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Ok(Some(pb));
        }
        anyhow::bail!("{env_var} does not exist: {p}");
    }
    Ok(which::which(bin_name).ok())
}

/// Resolve yt-dlp only (no ffmpeg) — used by the read-only probe path so it
/// doesn't pay for ffmpeg's large first-run download it never uses.
pub fn ensure_ytdlp(cfg: &Config) -> Result<PathBuf> {
    with_bin_lock(|bin| ytdlp::ensure(bin, cfg))
}

/// Resolve both tools, installing any that are missing. Blocking (network I/O);
/// call from async via `tokio::task::spawn_blocking`.
pub fn ensure(cfg: &Config) -> Result<Tools> {
    with_bin_lock(|bin| {
        let ytdlp = ytdlp::ensure(bin, cfg)?;
        let ffmpeg = ffmpeg::ensure(bin, cfg)?;
        let ffmpeg_dir = ffmpeg.parent().map(Path::to_path_buf);
        Ok(Tools {
            ytdlp,
            ffmpeg_dir,
            extractor_args: cfg.extractor_args.clone(),
        })
    })
}

/// Run `f` with the cache `bin` dir created and an exclusive lockfile held, so
/// concurrent first-run downloads across processes don't race.
fn with_bin_lock<T>(f: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let bin = cache_bin_dir();
    std::fs::create_dir_all(&bin).with_context(|| format!("create cache dir {}", bin.display()))?;

    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(bin.join(".lock"))?;
    // Best-effort: a filesystem without advisory locks (some network mounts)
    // shouldn't make tool resolution fail outright — downloads are still
    // individually atomic — but surface the degraded state instead of hiding it.
    if let Err(e) = lock.lock_exclusive() {
        tracing::warn!(error = %e, "could not acquire bootstrap lock; proceeding unsynchronized");
    }

    let result = f(&bin);

    let _ = FileExt::unlock(&lock);
    result
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
