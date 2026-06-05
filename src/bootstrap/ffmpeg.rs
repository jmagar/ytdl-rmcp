//! Resolve or install ffmpeg.
//!
//! Uses ffmpeg-sidecar's low-level download/unpack pointed at our own cache dir
//! (not its `auto_download`, which targets the executable's own directory).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ffmpeg_sidecar::download::{download_ffmpeg_package, ffmpeg_download_url, unpack_ffmpeg};

use super::{exe_name, resolve_override_or_path};
use crate::config::Config;

pub fn ensure(bin_dir: &Path, cfg: &Config) -> Result<PathBuf> {
    // 1. override / 2. PATH
    if let Some(p) = resolve_override_or_path(cfg.ffmpeg_path.as_deref(), "FFMPEG_PATH", "ffmpeg")?
    {
        return Ok(p);
    }
    // 3. cache
    let cached = bin_dir.join(exe_name("ffmpeg"));
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
