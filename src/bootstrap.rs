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

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use fs2::FileExt;
use sha2::{Digest, Sha256};

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
/// `tv/tootie/ytdl-rmcp` identity.
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("tv", "tootie", "ytdl-rmcp")
}

/// Per-user cache dir holding downloaded binaries (`<cache>/bin`).
pub fn cache_bin_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.cache_dir().join("bin"))
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-rmcp/bin"))
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

pub(crate) fn verify_sha256(path: &Path, expected: &str, label: &str) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    let actual = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != expected {
        bail!(
            "{label} checksum mismatch for {}: expected {expected}, got {actual}",
            path.display()
        );
    }
    Ok(())
}

/// Enforce an optional SHA-256 pin on `path`. When `expected` is `None` this is
/// a no-op; when set and the bytes don't match it's a hard error. Non-destructive
/// — used for user-supplied override/PATH binaries we must not delete.
pub(crate) fn verify_pin(path: &Path, expected: Option<&str>, label: &str) -> Result<()> {
    match expected {
        Some(expected) => verify_sha256(path, expected, label),
        None => Ok(()),
    }
}

/// Like [`verify_pin`], but for binaries this process just placed in the cache
/// (downloaded/unpacked). On a pin mismatch the offending file is removed
/// (best-effort) so a corrupt or tampered download is never cached and trusted
/// on a later run.
pub(crate) fn verify_pin_cached(path: &Path, expected: Option<&str>, label: &str) -> Result<()> {
    if let Err(e) = verify_pin(path, expected, label) {
        // Don't leave a poisoned binary in the cache for the next run to trust.
        if let Err(rm) = std::fs::remove_file(path) {
            tracing::warn!(
                error = %rm,
                path = %path.display(),
                "failed to remove file after checksum mismatch",
            );
        }
        return Err(e);
    }
    Ok(())
}

/// Where a tool resolved from, without triggering any download. Used by the
/// `doctor` diagnostic to report install state without paying for a first-run
/// download (which `ensure*` would otherwise perform).
#[derive(Debug, Clone)]
pub enum ResolvedTool {
    /// Found via `YTDLP_PATH`/`FFMPEG_PATH` override or on `PATH`.
    Found(PathBuf),
    /// Already present in the per-user cache `bin` dir.
    Cached(PathBuf),
    /// Not yet present anywhere — would be downloaded on first use.
    WouldBootstrap,
}

/// Read-only, download-free probe of where a tool *would* resolve from. Mirrors
/// the override → PATH → cache prefix of the real `ensure` resolvers but never
/// downloads and never mutates the cache. Intended for diagnostics only.
///
/// `override_path`/`env_var`/`bin_name` match a tool's `ensure` call (e.g.
/// `cfg.ytdlp_path`, `"YTDLP_PATH"`, `"yt-dlp"`). An override pointing at a
/// missing file surfaces as an error, just like the real path.
pub fn resolve_no_download(
    override_path: Option<&str>,
    env_var: &str,
    bin_name: &str,
) -> Result<ResolvedTool> {
    if let Some(found) = resolve_override_or_path(override_path, env_var, bin_name)? {
        return Ok(ResolvedTool::Found(found));
    }
    let cached = cache_bin_dir().join(exe_name(bin_name));
    if cached.is_file() {
        return Ok(ResolvedTool::Cached(cached));
    }
    Ok(ResolvedTool::WouldBootstrap)
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
    restrict_dir_perms(&bin);

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

/// Tighten the cache `bin` dir to owner-only (0o700) on Unix to shrink the
/// multi-user TOCTOU window between download/verify and exec. Best-effort: a
/// failure here shouldn't abort tool resolution. No-op on non-Unix.
#[cfg(unix)]
fn restrict_dir_perms(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
        tracing::warn!(error = %e, path = %dir.display(), "could not restrict cache dir permissions");
    }
}

#[cfg(not(unix))]
fn restrict_dir_perms(_dir: &Path) {}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
