//! High-level orchestration: resolve config, download, transfer, format.
//! Ports the body of `youtube_download` / `youtube_probe` from `server.py`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde_json::json;

use crate::bootstrap;
use crate::config::Config;
use crate::downloader::{self, ItemResult, ProbeResult};
use crate::model::{DownloadInput, ProbeInput, ResponseFormat};
use crate::urls::strip_mix_params;

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
    let video_dest = video_dest.unwrap_or_else(|| audio_dest.clone());

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
            input.mode,
            &staging_path,
            audio_format,
            &input.audio_quality,
            input.container,
            input.max_height,
            archive_dir.as_deref(),
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
        return Ok(render(
            &payload,
            input.response_format,
            render_download_markdown,
        ));
    }

    // The destination each kind actually produced files for — drives both the
    // transfer loop and the reported destination(s).
    let has_kind = |k: &str| results.iter().flat_map(|r| &r.files).any(|f| f.kind == k);
    let mut dests: Vec<(&str, &str)> = Vec::new();
    if has_kind("audio") {
        dests.push(("audio", &audio_dest));
    }
    if has_kind("video") {
        dests.push(("video", &video_dest));
    }

    // Transfer each kind to its own destination.
    let mut transfer_error: Option<String> = None;
    for (kind, dest) in &dests {
        let kind_dir = staging_path.join(kind);
        if !kind_dir.is_dir() {
            continue;
        }
        if let Err(e) = transfer_kind(&kind_dir, &remote, dest, &cfg.all_ssh_opts()).await {
            transfer_error = Some(e.to_string());
            break;
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
        &remote,
        &dests,
        transferred,
        transfer_error.clone(),
        staging_kept.as_deref(),
    );
    Ok(render(
        &payload,
        input.response_format,
        render_download_markdown,
    ))
}

async fn transfer_kind(dir: &Path, remote: &str, dest: &str, ssh_opts: &[String]) -> Result<()> {
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
        results.push(downloader::probe(&ytdlp, &url, cfg.extractor_args.as_deref()).await);
    }
    let payload = probe_payload(&results);
    Ok(render(
        &payload,
        input.response_format,
        render_probe_markdown,
    ))
}

// ── formatting ──────────────────────────────────────────────────────────────

fn render(
    payload: &serde_json::Value,
    fmt: ResponseFormat,
    md: fn(&serde_json::Value) -> String,
) -> String {
    match fmt {
        ResponseFormat::Json => serde_json::to_string_pretty(payload).unwrap_or_default(),
        ResponseFormat::Markdown => md(payload),
    }
}

fn download_payload(
    results: &[ItemResult],
    remote: &str,
    // (kind, dest_path) pairs that actually received files — so a video-only
    // download reports the video destination, and `both` reports both.
    dests: &[(&str, &str)],
    transferred: bool,
    transfer_error: Option<String>,
    staging_kept: Option<&Path>,
) -> serde_json::Value {
    let items: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            json!({
                "url": r.url,
                "title": r.title,
                "video_id": r.video_id,
                "duration": r.duration,
                "uploader": r.uploader,
                "is_playlist": r.is_playlist,
                "error": r.error,
                "files": r.files.iter().map(|f| json!({
                    "name": f.path.file_name().map(|n| n.to_string_lossy().to_string()),
                    "kind": f.kind,
                    "bytes": f.size,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let total_files: usize = results.iter().map(|r| r.files.len()).sum();
    let total_bytes: u64 = results.iter().flat_map(|r| &r.files).map(|f| f.size).sum();
    // Primary dest_path = first kind's path (audio in the common case); the full
    // per-kind breakdown is in `destinations`, and the human string lists all.
    let primary = dests.first().map(|(_, d)| *d).unwrap_or("");
    let destination = if dests.is_empty() {
        format!("{remote}:{primary}")
    } else {
        dests
            .iter()
            .map(|(_, d)| format!("{remote}:{d}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    json!({
        "transferred": transferred,
        "transfer_error": transfer_error,
        "remote": remote,
        "dest_path": primary,
        "destination": destination,
        "destinations": dests.iter().map(|(kind, d)| json!({
            "kind": kind, "dest_path": d, "destination": format!("{remote}:{d}"),
        })).collect::<Vec<_>>(),
        "staging_kept_at": staging_kept.map(|p| p.display().to_string()),
        "total_files": total_files,
        "total_bytes": total_bytes,
        "total_size": human_size(total_bytes),
        "items": items,
    })
}

fn probe_payload(results: &[ProbeResult]) -> serde_json::Value {
    json!({
        "items": results.iter().map(|r| json!({
            "url": r.url,
            "title": r.title,
            "video_id": r.video_id,
            "duration": r.duration,
            "uploader": r.uploader,
            "is_playlist": r.is_playlist,
            "entry_count": r.entry_count,
            "format_count": r.format_count,
            "error": r.error,
        })).collect::<Vec<_>>()
    })
}

fn render_download_markdown(p: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    let transferred = p["transferred"].as_bool().unwrap_or(false);
    if transferred {
        lines.push(format!(
            "Transferred {} file(s) ({}) to `{}`.",
            p["total_files"],
            p["total_size"].as_str().unwrap_or(""),
            p["destination"].as_str().unwrap_or("")
        ));
    } else {
        lines.push(format!(
            "Download succeeded but transfer failed: {}",
            p["transfer_error"].as_str().unwrap_or("unknown")
        ));
        if let Some(kept) = p["staging_kept_at"].as_str() {
            lines.push(format!("Local files kept at `{kept}` for retry."));
        }
    }
    lines.push(String::new());
    for item in p["items"].as_array().into_iter().flatten() {
        if let Some(err) = item["error"].as_str() {
            lines.push(format!(
                "- {} - failed: {err}",
                item["url"].as_str().unwrap_or("")
            ));
            continue;
        }
        let title = item["title"]
            .as_str()
            .unwrap_or_else(|| item["url"].as_str().unwrap_or(""));
        let suffix = if item["is_playlist"].as_bool().unwrap_or(false) {
            " (playlist)"
        } else {
            ""
        };
        let files = item["files"].as_array().cloned().unwrap_or_default();
        if files.is_empty() {
            lines.push(format!(
                "- {title}{suffix} - nothing new (already archived)"
            ));
            continue;
        }
        lines.push(format!("- {title}{suffix}"));
        for f in files {
            lines.push(format!(
                "    - [{}] {} ({})",
                f["kind"].as_str().unwrap_or(""),
                f["name"].as_str().unwrap_or(""),
                human_size(f["bytes"].as_u64().unwrap_or(0))
            ));
        }
    }
    lines.join("\n").trim().to_string()
}

fn render_probe_markdown(p: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    for r in p["items"].as_array().into_iter().flatten() {
        if let Some(err) = r["error"].as_str() {
            lines.push(format!(
                "- {} - failed: {err}",
                r["url"].as_str().unwrap_or("")
            ));
            continue;
        }
        let title = r["title"]
            .as_str()
            .unwrap_or_else(|| r["url"].as_str().unwrap_or(""));
        if r["is_playlist"].as_bool().unwrap_or(false) {
            lines.push(format!(
                "- {title} - playlist, {} item(s)",
                r["entry_count"]
            ));
        } else {
            let mut s = format!("- {title} - {}", human_duration(r["duration"].as_f64()));
            if let Some(up) = r["uploader"].as_str() {
                s.push_str(&format!(", by {up}"));
            }
            if let Some(fc) = r["format_count"].as_u64() {
                s.push_str(&format!(", {fc} formats"));
            }
            lines.push(s);
        }
    }
    lines.join("\n").trim().to_string()
}

fn human_size(bytes: u64) -> String {
    let mut size = bytes as f64;
    for unit in ["B", "KiB", "MiB", "GiB", "TiB"] {
        if size < 1024.0 || unit == "TiB" {
            return if unit == "B" {
                format!("{} B", bytes)
            } else {
                format!("{size:.1} {unit}")
            };
        }
        size /= 1024.0;
    }
    unreachable!()
}

fn human_duration(seconds: Option<f64>) -> String {
    let Some(s) = seconds.filter(|s| *s > 0.0) else {
        return "unknown".into();
    };
    let total = s as i64;
    let (h, rem) = (total / 3600, total % 3600);
    let (m, sec) = (rem / 60, rem % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}
