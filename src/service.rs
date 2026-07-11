//! High-level orchestration for the MCP tools: resolve the external tools,
//! download/probe/search via yt-dlp, transfer to the target, and format the
//! response payloads. Config is threaded as `Arc<Config>` so the blocking hops
//! (`spawn_blocking`) move a cheap refcount bump instead of deep-cloning all of
//! `Config`, and resolved tool paths are memoized per-process via [`ToolsCache`].

mod format;
mod plex_tracks;
mod retag;

use anyhow::{bail, Result};
#[cfg(not(windows))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::OnceCell;

use crate::bootstrap;
use crate::config::Config;
use crate::downloader::{self, FetchOptions, ItemResult};
use crate::model::{
    DownloadInput, IdentifyInput, ProbeInput, SearchInput, SearchPayload, StatsInput,
};
use crate::urls::strip_mix_params;

use format::{
    build_download_payload, probe_payload, render, render_download, render_probe_markdown,
    render_search_markdown,
};
use plex_tracks::plex_track_inputs;
use retag::auto_retag_audio;

/// Re-exported so the history ledger (`crate::history`) can consume the typed
/// download payload directly — `format` itself is a private submodule. The
/// item/file sub-structs are re-exported alongside so ledger tests can build a
/// representative payload without reaching into the private module path.
pub(crate) use format::DownloadPayload;
#[cfg(test)]
pub(crate) use format::{
    download_payload, render_download_markdown, DownloadFile, DownloadItem, DownloadStatus,
};

#[cfg(test)]
pub(crate) use format::render_search_for_test;

/// Re-exported for `service_tests.rs`, which drives the retag summary logic
/// through the test-injection seam without a live AcoustID round-trip.
#[cfg(test)]
pub(crate) use retag::auto_retag_audio_paths_for_test;

/// Process-lifetime cache of the resolved external tools. Bootstrap resolution
/// (`which`, file checks, the exclusive cross-process lockfile, and any first-run
/// download) is expensive and serializes concurrent calls, so we run it once per
/// process and reuse the result on every later call.
///
/// Two cells because the probe/search path needs yt-dlp only (it never pays for
/// ffmpeg's download), while the download path needs the full [`bootstrap::Tools`].
///
/// Cold cells fall through to `bootstrap::ensure*` (which takes the lock); warm
/// cells return the cached `Arc` with no lock and no re-resolution. When
/// `auto_update` is configured, caching is skipped entirely so the original
/// per-call freshness/update probe semantics are preserved.
#[derive(Default)]
pub struct ToolsCache {
    tools: OnceCell<Arc<bootstrap::Tools>>,
    ytdlp: OnceCell<Arc<PathBuf>>,
}

/// Default archive dir when `use_archive` is on but none configured.
fn default_archive_dir() -> PathBuf {
    bootstrap::project_dirs()
        .map(|d| d.state_dir().unwrap_or_else(|| d.data_dir()).to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-rmcp-state"))
}

pub async fn run_download(
    cfg: &Arc<Config>,
    cache: &ToolsCache,
    input: DownloadInput,
) -> Result<String> {
    let started = std::time::Instant::now();
    let legacy_target_path = legacy_ssh_target_path(
        input.remote.as_deref(),
        input.dest_path.as_deref(),
        cfg.target_path.as_deref(),
    );
    let target_path = input
        .target_path
        .clone()
        .or(legacy_target_path)
        .or_else(|| cfg.target_path.clone());
    let legacy_video_target_path = legacy_ssh_target_path(
        input.remote.as_deref(),
        input.video_dest_path.as_deref(),
        cfg.video_target_path
            .as_deref()
            .or(cfg.target_path.as_deref()),
    );
    let video_target_path = input
        .video_target_path
        .clone()
        .or(legacy_video_target_path)
        .or_else(|| cfg.video_target_path.clone())
        .or_else(|| target_path.clone());
    let Some(target_path) = target_path else {
        bail!("No target path. Pass 'target_path' or set YTDLP_TARGET_PATH.");
    };
    let target =
        crate::transfer::TransferTarget::parse_targets(&target_path, video_target_path.as_deref())?;
    if target.contains_local() && !cfg.allow_local_targets {
        bail!(
            "Local target paths are disabled. Set YTDLP_ALLOW_LOCAL_TARGETS=true to allow local filesystem destinations."
        );
    }

    let tools = ensure_tools(cfg, cache).await?;

    // BP-H2: archive + staging dir prep is blocking std::fs (create_dir_all,
    // tempfile). Offload it so it never runs on a reactor worker thread.
    let (archive_dir, staging) = prepare_dirs(cfg, input.use_archive).await?;
    let staging_path = staging.path().to_path_buf();

    // Audio codec: explicit arg, else the YTDLP_AUDIO_FORMAT env default.
    let audio_format = input
        .audio_format
        .unwrap_or_else(|| crate::model::AudioFormat::parse_or_default(&cfg.audio_format));

    // Download every URL (mix/radio cleaned first). Scheme-validate as a
    // defense-in-depth backstop (the `--` end-of-options guard at each yt-dlp
    // call site is the primary fix for option-injection).
    let validated_urls = input.urls.clone().into_validated_vec()?;
    tracing::info!(
        service = "ytdl-rmcp",
        action = "run_download",
        url_count = validated_urls.len(),
        mode = ?input.mode,
        target = %target.audio_target().display(),
        "download start"
    );
    let mut results: Vec<ItemResult> = Vec::new();
    for raw in validated_urls {
        let url = strip_mix_params(&raw);
        tracing::debug!(service = "ytdl-rmcp", action = "fetch", url = %url, mode = ?input.mode, "fetch start");
        let r = downloader::fetch(
            &tools,
            &url,
            FetchOptions {
                mode: input.mode,
                staging: &staging_path,
                audio_format,
                audio_quality: &input.audio_quality,
                container: input.container,
                max_height: input.max_height,
                archive_dir: archive_dir.as_deref(),
                timeout: Some(cfg.ytdlp_timeout()),
                clean_metadata: cfg.clean_metadata,
            },
        )
        .await;
        match r.error.as_deref() {
            None => {
                tracing::info!(service = "ytdl-rmcp", action = "fetch", url = %url, file_count = r.files.len(), "fetch complete")
            }
            Some(e) => {
                tracing::warn!(service = "ytdl-rmcp", action = "fetch", url = %url, error = %e, "fetch error")
            }
        }
        results.push(r);
    }

    let total_files: usize = results.iter().map(|r| r.files.len()).sum();
    if total_files == 0 {
        let errs: Vec<&str> = results.iter().filter_map(|r| r.error.as_deref()).collect();
        if !errs.is_empty() {
            bail!("Nothing was downloaded: {}", errs.join("; "));
        }
        // Archive hit / genuinely empty — succeed with a no-op summary.
        let noop_dest_strings = destination_strings_for_mode(input.mode, &target);
        let noop_dests: Vec<(&str, &str)> = noop_dest_strings
            .iter()
            .map(|(kind, dest)| (kind.as_str(), dest.as_str()))
            .collect();
        let mut payload = build_download_payload(&results, &noop_dests, true, None, None);
        record_plex_playlist(cfg, input.plex_playlist.clone(), &results, &mut payload).await;
        record_history(cfg, input.mode, &mut payload).await;
        return Ok(render_download(&payload, input.response_format));
    }

    // The destination each kind actually produced files for — drives both the
    // transfer loop and the reported destination(s).
    let has_kind = |k: &str| results.iter().flat_map(|r| &r.files).any(|f| f.kind == k);
    let mut transfer_dests: Vec<(&str, &crate::transfer::TargetPath)> = Vec::new();
    if has_kind("audio") {
        transfer_dests.push(("audio", target.audio_target()));
    }
    if has_kind("video") {
        transfer_dests.push(("video", target.video_target()));
    }
    let dest_strings: Vec<(String, String)> = transfer_dests
        .iter()
        .map(|(kind, dest)| ((*kind).to_string(), dest.display()))
        .collect();
    let dests: Vec<(&str, &str)> = dest_strings
        .iter()
        .map(|(kind, dest)| (kind.as_str(), dest.as_str()))
        .collect();

    let metadata_retag = auto_retag_audio(cfg, &results).await;

    // Transfer each kind to its own destination.
    let mut transfer_error: Option<String> = None;
    for (kind, dest) in &transfer_dests {
        let kind_dir = staging_path.join(kind);
        #[cfg(not(windows))]
        if !kind_dir.is_dir() {
            continue;
        }
        let ssh_opts = cfg.all_ssh_opts();
        tracing::info!(
            service = "ytdl-rmcp",
            action = "transfer",
            kind = %kind,
            target = %dest.display(),
            "transfer start"
        );
        #[cfg(windows)]
        let kind_files: Vec<PathBuf> = results
            .iter()
            .flat_map(|r| &r.files)
            .filter(|f| f.kind == *kind)
            .map(|f| f.path.clone())
            .collect();
        #[cfg(windows)]
        let transfer =
            crate::transfer::transfer_file_paths(&kind_files, &kind_dir, dest, &ssh_opts);
        #[cfg(not(windows))]
        let transfer = transfer_kind(&kind_dir, dest, &ssh_opts);
        match tokio::time::timeout(cfg.transfer_timeout(), transfer).await {
            Ok(Ok(())) => {
                tracing::info!(
                    service = "ytdl-rmcp",
                    action = "transfer",
                    kind = %kind,
                    target = %dest.display(),
                    "transfer success"
                );
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    service = "ytdl-rmcp",
                    action = "transfer",
                    kind = %kind,
                    error = %e,
                    "transfer error"
                );
                transfer_error = Some(e.to_string());
                break;
            }
            Err(_) => {
                let msg = format!(
                    "transfer of {kind} timed out after {}s",
                    cfg.transfer_timeout().as_secs()
                );
                tracing::warn!(
                    service = "ytdl-rmcp",
                    action = "transfer",
                    kind = %kind,
                    timeout_secs = cfg.transfer_timeout().as_secs(),
                    "transfer timeout"
                );
                transfer_error = Some(msg);
                break;
            }
        }
    }

    let transferred = transfer_error.is_none();
    // On transfer failure keep staging for retry; else clean unless keep_local.
    let keep = input.keep_local || !transferred;
    let staging_kept = if keep {
        // Persist the tempdir by leaking its path (don't auto-delete on drop).
        Some(staging.keep())
    } else {
        None
    };

    let mut payload = build_download_payload(
        &results,
        &dests,
        transferred,
        transfer_error.clone(),
        staging_kept.as_deref(),
    );
    payload.metadata_retag = metadata_retag;
    if transferred {
        record_plex_playlist(cfg, input.plex_playlist.clone(), &results, &mut payload).await;
    }
    record_history(cfg, input.mode, &mut payload).await;
    tracing::info!(
        service = "ytdl-rmcp",
        action = "run_download",
        mode = ?input.mode,
        transferred,
        elapsed_ms = started.elapsed().as_millis(),
        transfer_error = transfer_error.as_deref(),
        "download complete"
    );
    Ok(render_download(&payload, input.response_format))
}

fn legacy_ssh_target_path(
    remote_override: Option<&str>,
    path_override: Option<&str>,
    configured_target: Option<&str>,
) -> Option<String> {
    if remote_override.is_none() && path_override.is_none() {
        return None;
    }
    let configured = configured_target.and_then(ssh_parts);
    let remote = remote_override
        .map(str::to_string)
        .or_else(|| configured.as_ref().map(|(remote, _)| remote.clone()))?;
    let path = path_override
        .map(str::to_string)
        .or_else(|| configured.as_ref().map(|(_, path)| path.clone()))?;
    Some(format!("ssh:{remote}:{path}"))
}

fn ssh_parts(target: &str) -> Option<(String, String)> {
    match crate::transfer::TargetPath::parse(target).ok()? {
        crate::transfer::TargetPath::Ssh { remote, path } => {
            Some((remote.as_str().to_string(), path.as_str().to_string()))
        }
        _ => None,
    }
}

fn destination_strings_for_mode(
    mode: crate::model::DownloadMode,
    target: &crate::transfer::TransferTarget,
) -> Vec<(String, String)> {
    match mode {
        crate::model::DownloadMode::Audio => {
            vec![("audio".to_string(), target.audio_target().display())]
        }
        crate::model::DownloadMode::Video => {
            vec![("video".to_string(), target.video_target().display())]
        }
        crate::model::DownloadMode::Both => vec![
            ("audio".to_string(), target.audio_target().display()),
            ("video".to_string(), target.video_target().display()),
        ],
    }
}

#[cfg(not(windows))]
async fn transfer_kind(
    dir: &Path,
    target: &crate::transfer::TargetPath,
    ssh_opts: &[String],
) -> Result<()> {
    crate::transfer::ensure_target_dir(target, ssh_opts).await?;
    crate::transfer::transfer_to_target(dir, target, ssh_opts).await
}

/// BP-H2: prepare the archive + staging directories off the reactor. Both
/// `create_dir_all` and `tempfile::tempdir_in` are blocking std::fs calls.
async fn prepare_dirs(
    cfg: &Arc<Config>,
    use_archive: bool,
) -> Result<(Option<PathBuf>, tempfile::TempDir)> {
    let archive_cfg = cfg.archive_dir.clone();
    let staging_cfg = cfg.staging_dir.clone();
    tokio::task::spawn_blocking(move || {
        let archive_dir: Option<PathBuf> = if use_archive {
            let d = archive_cfg
                .map(PathBuf::from)
                .unwrap_or_else(default_archive_dir);
            std::fs::create_dir_all(&d)?;
            Some(d)
        } else {
            None
        };

        let staging_base = staging_cfg
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&staging_base)?;
        let staging = tempfile::Builder::new()
            .prefix("ytdlmcp_")
            .tempdir_in(&staging_base)?;
        Ok::<_, anyhow::Error>((archive_dir, staging))
    })
    .await?
}

/// Resolve/install yt-dlp + ffmpeg off the async runtime (blocking network I/O).
///
/// Memoized via [`ToolsCache`]: the first call runs `bootstrap::ensure` (which
/// takes the exclusive cross-process lock and may download); later calls reuse
/// the cached `Arc<Tools>` with no lock and no re-resolution. When `auto_update`
/// is on the cache is bypassed so the per-call freshness/update probe still runs.
async fn ensure_tools(cfg: &Arc<Config>, cache: &ToolsCache) -> Result<Arc<bootstrap::Tools>> {
    if cfg.auto_update {
        return resolve_tools(cfg).await;
    }
    cache
        .tools
        .get_or_try_init(|| resolve_tools(cfg))
        .await
        .cloned()
}

/// Cold-path resolution of the full toolset, off the reactor.
async fn resolve_tools(cfg: &Arc<Config>) -> Result<Arc<bootstrap::Tools>> {
    let cfg = Arc::clone(cfg);
    tokio::task::spawn_blocking(move || bootstrap::ensure(&cfg).map(Arc::new)).await?
}

/// Resolve yt-dlp only — probe never needs ffmpeg, so don't pay for its download.
///
/// Memoized like [`ensure_tools`], in its own cell (the probe/search path never
/// needs the full toolset). `auto_update` bypasses the cache identically.
async fn ensure_ytdlp(cfg: &Arc<Config>, cache: &ToolsCache) -> Result<Arc<PathBuf>> {
    if cfg.auto_update {
        return resolve_ytdlp(cfg).await;
    }
    cache
        .ytdlp
        .get_or_try_init(|| resolve_ytdlp(cfg))
        .await
        .cloned()
}

/// Cold-path resolution of yt-dlp only, off the reactor.
async fn resolve_ytdlp(cfg: &Arc<Config>) -> Result<Arc<PathBuf>> {
    let cfg = Arc::clone(cfg);
    tokio::task::spawn_blocking(move || bootstrap::ensure_ytdlp(&cfg).map(Arc::new)).await?
}

pub async fn run_probe(cfg: &Arc<Config>, cache: &ToolsCache, input: ProbeInput) -> Result<String> {
    let started = std::time::Instant::now();
    let ytdlp = ensure_ytdlp(cfg, cache).await?;
    let validated_urls = input.urls.into_validated_vec()?;
    tracing::info!(
        service = "ytdl-rmcp",
        action = "run_probe",
        url_count = validated_urls.len(),
        "probe start"
    );
    let mut results = Vec::new();
    for raw in validated_urls {
        let url = strip_mix_params(&raw);
        tracing::debug!(service = "ytdl-rmcp", action = "probe", url = %url, "probe url start");
        let result = downloader::probe(
            &ytdlp,
            &url,
            cfg.extractor_args.as_deref(),
            Some(cfg.ytdlp_timeout()),
        )
        .await;
        match result.error.as_deref() {
            None => tracing::info!(
                service = "ytdl-rmcp",
                action = "probe",
                url = %url,
                title = result.title.as_deref().unwrap_or("(unknown)"),
                duration_s = result.duration.unwrap_or(0.0),
                "probe url success"
            ),
            Some(e) => {
                tracing::warn!(service = "ytdl-rmcp", action = "probe", url = %url, error = %e, "probe url error")
            }
        }
        results.push(result);
    }
    let payload = probe_payload(&results);
    tracing::info!(
        service = "ytdl-rmcp",
        action = "run_probe",
        elapsed_ms = started.elapsed().as_millis(),
        "probe complete"
    );
    Ok(render(
        &payload,
        input.response_format,
        render_probe_markdown,
    ))
}

pub async fn run_identify(cfg: &Arc<Config>, input: IdentifyInput) -> Result<String> {
    let payload =
        crate::identify::identify_files(cfg, input.paths.into_vec(), input.write_tags).await?;
    Ok(render(
        &serde_json::to_value(&payload)?,
        input.response_format,
        crate::identify::render_identify_markdown,
    ))
}

pub async fn run_search_payload(
    cfg: &Arc<Config>,
    cache: &ToolsCache,
    input: &SearchInput,
) -> Result<SearchPayload> {
    let started = std::time::Instant::now();
    let query = input.query.trim();
    if query.is_empty() {
        bail!("Search query cannot be empty.");
    }

    let ytdlp = ensure_ytdlp(cfg, cache).await?;
    let limit = input.effective_limit();
    tracing::info!(service = "ytdl-rmcp", action = "run_search", query = %query, limit, "search start");
    let results = downloader::search_youtube(
        &ytdlp,
        query,
        limit,
        cfg.extractor_args.as_deref(),
        Some(cfg.ytdlp_timeout()),
    )
    .await?;

    tracing::info!(
        service = "ytdl-rmcp",
        action = "run_search",
        query = %query,
        result_count = results.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "search complete"
    );
    Ok(SearchPayload {
        query: query.to_string(),
        limit,
        results,
    })
}

pub async fn run_search(
    cfg: &Arc<Config>,
    cache: &ToolsCache,
    input: SearchInput,
) -> Result<String> {
    let payload = run_search_payload(cfg, cache, &input).await?;
    Ok(render(
        &serde_json::to_value(&payload)?,
        input.response_format,
        render_search_markdown,
    ))
}

/// Append the download to the persistent ledger, off the reactor.
///
/// `append_download` does synchronous `std::fs` I/O (directory create + JSONL
/// append), so it is offloaded via `spawn_blocking` — mirroring `prepare_dirs`
/// and `record_plex_playlist`. A cheap clone of the payload moves into the
/// closure; on failure (best-effort) the error is recorded back onto the live
/// payload so the response still reports `history_error`.
async fn record_history(
    cfg: &Arc<Config>,
    mode: crate::model::DownloadMode,
    payload: &mut DownloadPayload,
) {
    let cfg = Arc::clone(cfg);
    let snapshot = payload.clone();
    let result =
        tokio::task::spawn_blocking(move || crate::history::append_download(&cfg, mode, &snapshot))
            .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(%error, "failed to append download history");
            payload.history_error = Some(error.to_string());
        }
        Err(error) => {
            tracing::warn!(%error, "download history task failed");
            payload.history_error = Some(error.to_string());
        }
    }
}

async fn record_plex_playlist(
    cfg: &Arc<Config>,
    requested_playlist: Option<String>,
    results: &[ItemResult],
    payload: &mut DownloadPayload,
) {
    let Some(playlist) = requested_playlist
        .or_else(|| cfg.plex_playlist.clone())
        .filter(|s| !s.trim().is_empty())
    else {
        return;
    };
    let cfg = Arc::clone(cfg);
    // Map the downloader's results into Plex's own input DTO here, so the Plex
    // integration never depends on `downloader::ItemResult`'s shape.
    let tracks = plex_track_inputs(results);
    match tokio::task::spawn_blocking(move || {
        crate::plex::add_downloaded_audio(&cfg, &playlist, &tracks)
    })
    .await
    {
        Ok(Ok(update)) => {
            payload.plex_playlist = Some(update);
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "failed to update Plex playlist");
            payload.plex_playlist_error = Some(error.to_string());
        }
        Err(error) => {
            tracing::warn!(%error, "Plex playlist task failed");
            payload.plex_playlist_error = Some(error.to_string());
        }
    }
}

pub fn run_stats(cfg: &Config, input: StatsInput) -> Result<String> {
    let payload = crate::history::stats_payload(cfg, input.effective_limit())?;
    Ok(render(
        &payload,
        input.response_format,
        crate::history::render_stats_markdown,
    ))
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "service/stats_identify_tests.rs"]
mod stats_identify_tests;

#[cfg(test)]
#[path = "service/render_tests.rs"]
mod render_tests;
