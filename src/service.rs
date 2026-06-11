//! High-level orchestration: resolve config, download, transfer, format.
//! Ports the body of `youtube_download` / `youtube_probe` from `server.py`.

mod format;

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::bootstrap;
use crate::config::Config;
use crate::downloader::{self, FetchOptions, ItemResult};
use crate::model::{DownloadInput, ProbeInput, SearchInput, SearchPayload, StatsInput};
use crate::urls::strip_mix_params;

use format::{
    download_payload, probe_payload, render, render_download_markdown, render_probe_markdown,
    render_search_markdown,
};

#[cfg(test)]
pub(crate) use format::render_search_for_test;

/// Default archive dir when `use_archive` is on but none configured.
fn default_archive_dir() -> PathBuf {
    bootstrap::project_dirs()
        .map(|d| d.state_dir().unwrap_or_else(|| d.data_dir()).to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-mcp-state"))
}

pub async fn run_download(cfg: &Config, input: DownloadInput) -> Result<String> {
    let remote = input.remote.clone().or_else(|| cfg.remote.clone());
    let audio_dest = input.dest_path.clone().or_else(|| cfg.dest_path.clone());
    let video_dest = input
        .video_dest_path
        .clone()
        .or_else(|| cfg.video_dest_path.clone())
        .or_else(|| audio_dest.clone());

    let Some(remote) = remote else {
        bail!("No SSH remote. Pass 'remote' or set the YTDLP_REMOTE env var.");
    };
    let Some(audio_dest) = audio_dest else {
        bail!("No destination. Pass 'dest_path' or set YTDLP_REMOTE_PATH.");
    };
    let target =
        crate::transfer::TransferTarget::parse(&remote, &audio_dest, video_dest.as_deref())?;

    let tools = ensure_tools(cfg).await?;

    let archive_dir: Option<PathBuf> = if input.use_archive {
        let d = cfg
            .archive_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(default_archive_dir);
        std::fs::create_dir_all(&d)?;
        Some(d)
    } else {
        None
    };

    let staging_base = cfg
        .staging_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&staging_base)?;
    let staging = tempfile::Builder::new()
        .prefix("ytdlmcp_")
        .tempdir_in(&staging_base)?;
    let staging_path = staging.path().to_path_buf();

    // Audio codec: explicit arg, else the YTDLP_AUDIO_FORMAT env default.
    let audio_format = input
        .audio_format
        .unwrap_or_else(|| crate::model::AudioFormat::parse_or_default(&cfg.audio_format));

    // Download every URL (mix/radio cleaned first).
    let mut results: Vec<ItemResult> = Vec::new();
    for raw in input.urls.clone().into_vec() {
        let url = strip_mix_params(&raw);
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
            },
        )
        .await;
        results.push(r);
    }

    let total_files: usize = results.iter().map(|r| r.files.len()).sum();
    if total_files == 0 {
        let errs: Vec<&str> = results.iter().filter_map(|r| r.error.as_deref()).collect();
        if !errs.is_empty() {
            bail!("Nothing was downloaded: {}", errs.join("; "));
        }
        // Archive hit / genuinely empty — succeed with a no-op summary.
        let payload = download_payload(&results, &remote, &[], true, None, None);
        crate::history::append_download(cfg, input.mode, &payload)?;
        return Ok(render(
            &payload,
            input.response_format,
            render_download_markdown,
        ));
    }

    // The destination each kind actually produced files for — drives both the
    // transfer loop and the reported destination(s).
    let has_kind = |k: &str| results.iter().flat_map(|r| &r.files).any(|f| f.kind == k);
    let mut transfer_dests: Vec<(&str, &crate::transfer::RemotePath)> = Vec::new();
    if has_kind("audio") {
        transfer_dests.push(("audio", target.audio_dest()));
    }
    if has_kind("video") {
        transfer_dests.push(("video", target.video_dest()));
    }
    let dests: Vec<(&str, &str)> = transfer_dests
        .iter()
        .map(|(kind, dest)| (*kind, dest.as_str()))
        .collect();

    // Transfer each kind to its own destination.
    let mut transfer_error: Option<String> = None;
    for (kind, dest) in &transfer_dests {
        let kind_dir = staging_path.join(kind);
        if !kind_dir.is_dir() {
            continue;
        }
        let ssh_opts = cfg.all_ssh_opts();
        let transfer = transfer_kind(&kind_dir, target.remote(), dest, &ssh_opts);
        match tokio::time::timeout(cfg.transfer_timeout(), transfer).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                transfer_error = Some(e.to_string());
                break;
            }
            Err(_) => {
                transfer_error = Some(format!(
                    "transfer of {kind} timed out after {}s",
                    cfg.transfer_timeout().as_secs()
                ));
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

    let payload = download_payload(
        &results,
        target.remote().as_str(),
        &dests,
        transferred,
        transfer_error.clone(),
        staging_kept.as_deref(),
    );
    crate::history::append_download(cfg, input.mode, &payload)?;
    Ok(render(
        &payload,
        input.response_format,
        render_download_markdown,
    ))
}

async fn transfer_kind(
    dir: &Path,
    remote: &crate::transfer::RemoteSpec,
    dest: &crate::transfer::RemotePath,
    ssh_opts: &[String],
) -> Result<()> {
    crate::transfer::ensure_remote_dir(remote, dest, ssh_opts).await?;
    crate::transfer::transfer(dir, remote, dest, ssh_opts).await
}

/// Resolve/install yt-dlp + ffmpeg off the async runtime (blocking network I/O).
async fn ensure_tools(cfg: &Config) -> Result<bootstrap::Tools> {
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || bootstrap::ensure(&cfg)).await?
}

/// Resolve yt-dlp only — probe never needs ffmpeg, so don't pay for its download.
async fn ensure_ytdlp(cfg: &Config) -> Result<PathBuf> {
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || bootstrap::ensure_ytdlp(&cfg)).await?
}

pub async fn run_probe(cfg: &Config, input: ProbeInput) -> Result<String> {
    let ytdlp = ensure_ytdlp(cfg).await?;
    let mut results = Vec::new();
    for raw in input.urls.into_vec() {
        let url = strip_mix_params(&raw);
        results.push(
            downloader::probe(
                &ytdlp,
                &url,
                cfg.extractor_args.as_deref(),
                Some(cfg.ytdlp_timeout()),
            )
            .await,
        );
    }
    let payload = probe_payload(&results);
    Ok(render(
        &payload,
        input.response_format,
        render_probe_markdown,
    ))
}

pub async fn run_search_payload(cfg: &Config, input: &SearchInput) -> Result<SearchPayload> {
    let query = input.query.trim();
    if query.is_empty() {
        bail!("Search query cannot be empty.");
    }

    let ytdlp = ensure_ytdlp(cfg).await?;
    let limit = input.effective_limit();
    let results = downloader::search_youtube(
        &ytdlp,
        query,
        limit,
        cfg.extractor_args.as_deref(),
        Some(cfg.ytdlp_timeout()),
    )
    .await?;

    Ok(SearchPayload {
        query: query.to_string(),
        limit,
        results,
    })
}

pub async fn run_search(cfg: &Config, input: SearchInput) -> Result<String> {
    let payload = run_search_payload(cfg, &input).await?;
    Ok(render(
        &serde_json::to_value(&payload)?,
        input.response_format,
        render_search_markdown,
    ))
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
