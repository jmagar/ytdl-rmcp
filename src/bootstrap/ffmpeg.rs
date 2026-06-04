//! Resolve or install ffmpeg.
//!
//! Uses ffmpeg-sidecar's low-level download/unpack pointed at our own cache dir
//! (not its `auto_download`, which targets the executable's own directory).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ffmpeg_sidecar::download::{download_ffmpeg_package, ffmpeg_download_url, unpack_ffmpeg};

use crate::config::Config;

fn local_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

pub fn ensure(bin_dir: &Path, cfg: &Config) -> Result<PathBuf> {
    // 1. explicit override
    if let Some(p) = &cfg.ffmpeg_path {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Ok(pb);
        }
        anyhow::bail!("FFMPEG_PATH does not exist: {p}");
    }
    // 2. PATH
    if let Ok(p) = which::which("ffmpeg") {
        return Ok(p);
    }
    // 3. cache
    let cached = bin_dir.join(local_name());
    if cached.is_file() {
        return Ok(cached);
    }
    // 4. download + unpack into the cache dir
    let url = ffmpeg_download_url().context("no ffmpeg build for this platform")?;
    tracing::info!(%url, "downloading ffmpeg");
    let archive = download_ffmpeg_package(url, bin_dir).context("download ffmpeg package")?;
    unpack_ffmpeg(&archive, bin_dir).context("unpack ffmpeg")?;
    if cached.is_file() {
        Ok(cached)
    } else {
        anyhow::bail!("ffmpeg not found at {} after unpack", cached.display())
    }
}
