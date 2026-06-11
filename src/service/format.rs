use std::path::Path;

use serde_json::json;

use crate::downloader::{ItemResult, ProbeResult};
use crate::model::ResponseFormat;

#[cfg(test)]
use crate::model::SearchPayload;

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

pub(crate) fn download_payload(
    results: &[ItemResult],
    remote: &str,
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
                "status": item_status(r),
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
    let partial_items = results
        .iter()
        .filter(|r| r.error.is_some() && !r.files.is_empty())
        .count();
    let failed_items = results
        .iter()
        .filter(|r| r.error.is_some() && r.files.is_empty())
        .count();
    let primary = dests.first().map(|(_, d)| *d).unwrap_or("");
    let destination: Option<String> = if dests.is_empty() {
        None
    } else {
        Some(
            dests
                .iter()
                .map(|(_, d)| format!("{remote}:{d}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
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
        "partial_items": partial_items,
        "failed_items": failed_items,
        "items": items,
    })
}

fn item_status(result: &ItemResult) -> &'static str {
    match (result.error.is_some(), result.files.is_empty()) {
        (true, true) => "failed",
        (true, false) => "partial",
        (false, true) => "skipped",
        (false, false) => "ok",
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
