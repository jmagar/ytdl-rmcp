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
use fs2::FileExt;

use crate::config::Config;

/// Resolved paths to the external tools used by a download.
#[derive(Debug, Clone)]
pub struct Tools {
    pub ytdlp: PathBuf,
    /// Directory containing ffmpeg (passed to yt-dlp via `--ffmpeg-location`),
    /// or None to let yt-dlp find ffmpeg itself.
    pub ffmpeg_dir: Option<PathBuf>,
}

/// Per-user cache dir holding downloaded binaries (`<cache>/bin`).
pub fn cache_bin_dir() -> PathBuf {
    directories::ProjectDirs::from("tv", "tootie", "ytdl-mcp")
        .map(|d| d.cache_dir().join("bin"))
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-mcp/bin"))
}

/// Resolve both tools, installing any that are missing. Blocking (network I/O);
/// call from async via `tokio::task::spawn_blocking`.
pub fn ensure(cfg: &Config) -> Result<Tools> {
    let bin = cache_bin_dir();
    std::fs::create_dir_all(&bin).with_context(|| format!("create cache dir {}", bin.display()))?;

    // Serialize concurrent first-run downloads across processes.
    let lock_path = bin.join(".lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.lock_exclusive().ok();

    let result = (|| {
        let ytdlp = ytdlp::ensure(&bin, cfg).context(
            "could not find or install yt-dlp. Set YTDLP_PATH, put it on PATH, or check network.",
        )?;
        let ffmpeg = ffmpeg::ensure(&bin, cfg)?;
        let ffmpeg_dir = ffmpeg.parent().map(Path::to_path_buf);
        Ok(Tools { ytdlp, ffmpeg_dir })
    })();

    let _ = FileExt::unlock(&lock);
    result
}
