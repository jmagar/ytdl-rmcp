use std::path::Path;

use serde::Serialize;
use serde_json::json;

use crate::downloader::{ItemResult, ProbeResult};
use crate::model::ResponseFormat;

#[cfg(test)]
use crate::model::SearchPayload;

/// Typed source-of-truth for the `youtube_download` result payload.
///
/// `target_path` is the current destination contract. Deprecated `remote` and
/// `dest_path` fields are retained as compatibility fields for SSH-shaped
/// targets during the migration from `YTDLP_REMOTE` + `YTDLP_REMOTE_PATH`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadPayload {
    pub transferred: bool,
    pub transfer_error: Option<String>,
    pub remote: Option<String>,
    pub dest_path: String,
    pub target_path: String,
    pub destination: Option<String>,
    pub destinations: Vec<DownloadDestination>,
    pub staging_kept_at: Option<String>,
    pub total_files: usize,
    pub total_bytes: u64,
    pub total_size: String,
    pub partial_items: usize,
    pub failed_items: usize,
    pub items: Vec<DownloadItem>,
    // Side-channels attached after the core build in service.rs. Only present
    // when the corresponding step ran/failed, hence skip-if-none. Both are typed
    // (not `serde_json::Value`): `RetagSummary` and `PlexPlaylistUpdate` own the
    // schema, so their serialized JSON stays byte-compatible with the previous
    // hand-built maps while field renames become compile errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_retag: Option<super::retag::RetagSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plex_playlist: Option<crate::plex::PlexPlaylistUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plex_playlist_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadDestination {
    pub kind: String,
    pub dest_path: String,
    pub target_path: String,
    pub destination: String,
}

/// Per-item terminal status. Serializes to the exact lowercase strings the
/// previous `&'static str` field emitted (`"ok"`/`"partial"`/`"skipped"`/
/// `"failed"`), so the JSON output stays byte-identical — but the contract is
/// now closed over a fixed set of variants instead of being stringly-typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DownloadStatus {
    Ok,
    Partial,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadItem {
    pub url: String,
    pub status: DownloadStatus,
    pub title: Option<String>,
    pub video_id: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub is_playlist: bool,
    pub error: Option<String>,
    pub files: Vec<DownloadFile>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadFile {
    pub name: Option<String>,
    pub kind: &'static str,
    pub bytes: u64,
    pub title: Option<String>,
    pub video_id: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
}

pub(crate) fn render(
    payload: &serde_json::Value,
    fmt: ResponseFormat,
    md: fn(&serde_json::Value) -> String,
) -> String {
    match fmt {
        ResponseFormat::Json => serde_json::to_string_pretty(payload).unwrap_or_default(),
        ResponseFormat::Markdown => md(payload),
    }
}

/// Render the typed download payload. The typed struct is the source of truth;
/// it is serialized to a `Value` exactly once here, at the rendering boundary,
/// for both the JSON output and the (still `Value`-based) markdown renderer.
pub(crate) fn render_download(payload: &DownloadPayload, fmt: ResponseFormat) -> String {
    let value = serde_json::to_value(payload).unwrap_or_default();
    render(&value, fmt, render_download_markdown)
}

/// Build the typed download payload. This is the schema source of truth used by
/// the orchestrator (`service.rs`), the markdown renderer, and the history
/// ledger; field renames are now compile-checked at every consumer.
pub(crate) fn build_download_payload(
    results: &[ItemResult],
    dests: &[(&str, &str)],
    transferred: bool,
    transfer_error: Option<String>,
    staging_kept: Option<&Path>,
) -> DownloadPayload {
    let items: Vec<DownloadItem> = results
        .iter()
        .map(|r| DownloadItem {
            url: r.url.clone(),
            status: item_status(r),
            title: r.title.clone(),
            video_id: r.video_id.clone(),
            duration: r.duration,
            uploader: r.uploader.clone(),
            is_playlist: r.is_playlist,
            error: r.error.clone(),
            files: r
                .files
                .iter()
                .map(|f| DownloadFile {
                    name: f.path.file_name().map(|n| n.to_string_lossy().to_string()),
                    kind: f.kind,
                    bytes: f.size,
                    title: f.title.clone(),
                    video_id: f.video_id.clone(),
                    uploader: f.uploader.clone(),
                    duration: f.duration,
                })
                .collect(),
        })
        .collect();
    let total_files: usize = results.iter().map(|r| r.files.len()).sum();
    let total_bytes: u64 = results.iter().flat_map(|r| &r.files).map(|f| f.size).sum();
    let partial_items = results
        .iter()
        .filter(|r| r.error.is_some() && !r.files.is_empty())
        .count();
    let failed_items = results
        .iter()
        .filter(|r| r.error.is_some() && r.files.is_empty())
        .count();
    let primary = dests.first().map(|(_, d)| *d).unwrap_or("");
    let (remote, dest_path) = legacy_remote_dest(primary);
    let destination: Option<String> = if dests.is_empty() {
        None
    } else {
        Some(
            dests
                .iter()
                .map(|(_, d)| (*d).to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    DownloadPayload {
        transferred,
        transfer_error,
        remote,
        dest_path,
        target_path: primary.to_string(),
        destination,
        destinations: dests
            .iter()
            .map(|(kind, d)| DownloadDestination {
                kind: (*kind).to_string(),
                dest_path: legacy_remote_dest(d).1,
                target_path: (*d).to_string(),
                destination: (*d).to_string(),
            })
            .collect(),
        staging_kept_at: staging_kept.map(|p| p.display().to_string()),
        total_files,
        total_bytes,
        total_size: human_size(total_bytes),
        partial_items,
        failed_items,
        items,
        metadata_retag: None,
        plex_playlist: None,
        plex_playlist_error: None,
        history_error: None,
    }
}

fn legacy_remote_dest(target: &str) -> (Option<String>, String) {
    if target.starts_with("rclone:") {
        return (None, target.to_string());
    }
    if let Some((remote, path)) = target.split_once(":/") {
        return (Some(remote.to_string()), format!("/{path}"));
    }
    (None, target.to_string())
}

/// `Value` view of [`build_download_payload`], retained for tests that assert on
/// the rendered JSON shape by key. Production code uses the typed builder.
#[cfg(test)]
pub(crate) fn download_payload(
    results: &[ItemResult],
    dests: &[(&str, &str)],
    transferred: bool,
    transfer_error: Option<String>,
    staging_kept: Option<&Path>,
) -> serde_json::Value {
    serde_json::to_value(build_download_payload(
        results,
        dests,
        transferred,
        transfer_error,
        staging_kept,
    ))
    .expect("download payload serializes")
}

fn item_status(result: &ItemResult) -> DownloadStatus {
    match (result.error.is_some(), result.files.is_empty()) {
        (true, true) => DownloadStatus::Failed,
        (true, false) => DownloadStatus::Partial,
        (false, true) => DownloadStatus::Skipped,
        (false, false) => DownloadStatus::Ok,
    }
}

pub(crate) fn probe_payload(results: &[ProbeResult]) -> serde_json::Value {
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

pub(crate) fn render_download_markdown(p: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    let transferred = p["transferred"].as_bool().unwrap_or(false);
    let total_files = p["total_files"].as_u64().unwrap_or(0);
    if transferred && total_files == 0 {
        lines.push("Nothing new to download (already archived).".to_string());
    } else if transferred {
        lines.push(format!(
            "Transferred {} file(s) ({}) to `{}`.",
            total_files,
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
        let files = item["files"].as_array().cloned().unwrap_or_default();
        if let Some(err) = item["error"].as_str().filter(|_| files.is_empty()) {
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
        if files.is_empty() {
            lines.push(format!(
                "- {title}{suffix} - nothing new (already archived)"
            ));
            continue;
        }
        if let Some(err) = item["error"].as_str() {
            lines.push(format!("- {title}{suffix} - partially completed: {err}"));
        } else {
            lines.push(format!("- {title}{suffix}"));
        }
        for f in files {
            lines.push(format!(
                "    - [{}] {} ({})",
                f["kind"].as_str().unwrap_or(""),
                f["name"].as_str().unwrap_or(""),
                human_size(f["bytes"].as_u64().unwrap_or(0))
            ));
        }
    }
    if let Some(plex) = p["plex_playlist"].as_object() {
        let playlist = plex
            .get("playlist")
            .and_then(|v| v.as_str())
            .unwrap_or("Plex playlist");
        lines.push(String::new());
        lines.push(format!(
            "Plex `{playlist}`: matched {}, added {}, already present {}, missing {}.",
            plex.get("matched").and_then(|v| v.as_u64()).unwrap_or(0),
            plex.get("added").and_then(|v| v.as_u64()).unwrap_or(0),
            plex.get("already_present")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            plex.get("missing")
                .and_then(|v| v.as_array())
                .map(Vec::len)
                .unwrap_or(0)
        ));
    }
    if let Some(retag) = p["metadata_retag"].as_object() {
        lines.push(String::new());
        if let Some(error) = retag.get("error").and_then(|v| v.as_str()) {
            lines.push(format!("Metadata retagging failed: {error}"));
        } else {
            lines.push(format!(
                "Metadata retagging: scanned {}, matched {}, wrote {}, skipped {}, errors {}.",
                retag.get("attempted").and_then(|v| v.as_u64()).unwrap_or(0),
                retag.get("matched").and_then(|v| v.as_u64()).unwrap_or(0),
                retag.get("written").and_then(|v| v.as_u64()).unwrap_or(0),
                retag.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0),
                retag.get("errors").and_then(|v| v.as_u64()).unwrap_or(0)
            ));
        }
    }
    if let Some(error) = p["plex_playlist_error"].as_str() {
        lines.push(String::new());
        lines.push(format!("Plex playlist update failed: {error}"));
    }
    lines.join("\n").trim().to_string()
}

pub(crate) fn render_probe_markdown(p: &serde_json::Value) -> String {
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
                r["entry_count"].as_u64().unwrap_or(0)
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

pub(crate) fn render_search_markdown(payload: &serde_json::Value) -> String {
    let query = payload["query"].as_str().unwrap_or("");
    let mut out = format!("# YouTube search: {query}\n\n");
    let Some(results) = payload["results"].as_array() else {
        return out;
    };
    if results.is_empty() {
        out.push_str("No results.\n");
        return out;
    }

    for (idx, item) in results.iter().enumerate() {
        let title = item["title"].as_str().unwrap_or("Untitled");
        let url = item["url"].as_str().unwrap_or("");
        let uploader = item["uploader"].as_str().unwrap_or("Unknown channel");
        let duration = item["duration"]
            .as_f64()
            .map(format_duration)
            .unwrap_or_else(|| "unknown duration".into());
        out.push_str(&format!(
            "{}. [{}]({})\n   - {} - {}\n",
            idx + 1,
            title,
            url,
            uploader,
            duration
        ));
    }
    out
}

#[cfg(test)]
pub(crate) fn render_search_for_test(payload: &SearchPayload, format: ResponseFormat) -> String {
    render(
        &serde_json::to_value(payload).expect("search payload serializes"),
        format,
        render_search_markdown,
    )
}

fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
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
