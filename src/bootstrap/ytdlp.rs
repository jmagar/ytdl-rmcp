//! Resolve or install yt-dlp.
//!
//! yt-dlp ships self-contained PyInstaller binaries on GitHub releases that
//! need no system Python: `yt-dlp_linux`, `yt-dlp.exe`, `yt-dlp_macos`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;

use super::{exe_name, http, resolve_override_or_path};
use crate::config::Config;

const STABLE_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const NIGHTLY_BASE: &str =
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download";

/// GitHub release asset for the current platform.
fn asset_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp_linux"
    }
}

pub fn ensure(bin_dir: &Path, cfg: &Config) -> Result<PathBuf> {
    // 1. override / 2. PATH
    if let Some(p) = resolve_override_or_path(cfg.ytdlp_path.as_deref(), "YTDLP_PATH", "yt-dlp")? {
        return Ok(p);
    }
    // 3 / 4. cache, downloading or refreshing as needed
    let cached = bin_dir.join(exe_name("yt-dlp"));
    if cached.is_file() && fresh_enough(&cached, cfg) {
        return Ok(cached);
    }
    let base = if cfg.update_pre {
        NIGHTLY_BASE
    } else {
        STABLE_BASE
    };
    let url = format!("{base}/{}", asset_name());
    tracing::info!(%url, "downloading yt-dlp");
    http::download_to_file(&url, &cached)?;
    make_executable(&cached)?;
    Ok(cached)
}

/// True when the cached binary should be kept (auto-update off, or younger than
/// the staleness threshold).
fn fresh_enough(path: &Path, cfg: &Config) -> bool {
    if !cfg.auto_update {
        return true;
    }
    let max_age = Duration::from_secs((cfg.max_age_days.max(0) as u64) * 86_400);
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|mtime| SystemTime::now().duration_since(mtime).unwrap_or_default() < max_age)
        .unwrap_or(false)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}
