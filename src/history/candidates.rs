use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::history::{history_path, HistoryLock};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct PlaylistCandidatesPayload {
    pub history_path: String,
    pub skipped_entries: u64,
    pub candidates: Vec<PlaylistCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct PlaylistCandidate {
    pub candidate_id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub video_id: Option<String>,
    pub url: String,
    pub timestamp: String,
    pub duration: Option<f64>,
    pub bytes: u64,
}

pub(crate) fn playlist_candidates(cfg: &Config, limit: usize) -> Result<PlaylistCandidatesPayload> {
    let path = history_path(cfg);
    let _guard = HistoryLock::acquire(&path);
    let file = match File::open(&path) {
        Ok(file) => Some(file),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("open history file {}", path.display()));
        }
    };

    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut skipped_entries = 0_u64;

    if let Some(file) = file {
        for line in BufReader::new(file).lines() {
            let line = line.with_context(|| format!("read history file {}", path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: Value = match serde_json::from_str(&line) {
                Ok(entry) => entry,
                Err(error) => {
                    skipped_entries += 1;
                    tracing::warn!(%error, "skipping malformed download history entry");
                    continue;
                }
            };
            if entry["transferred"].as_bool() != Some(true) {
                continue;
            }
            let timestamp = entry["timestamp"].as_str().unwrap_or("").to_string();
            for item in entry["items"].as_array().into_iter().flatten() {
                collect_item_candidates(item, &timestamp, &mut seen, &mut candidates);
            }
        }
    }

    candidates.reverse();
    if limit > 0 && candidates.len() > limit {
        candidates.truncate(limit);
    }
    Ok(payload(path, skipped_entries, candidates))
}

fn collect_item_candidates(
    item: &Value,
    timestamp: &str,
    seen: &mut BTreeSet<String>,
    candidates: &mut Vec<PlaylistCandidate>,
) {
    let url = item["url"].as_str().unwrap_or("").to_string();
    for file in item["files"].as_array().into_iter().flatten() {
        if file["kind"].as_str() != Some("audio") {
            continue;
        }
        let title = file["title"]
            .as_str()
            .or_else(|| item["title"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }
        let uploader = file["uploader"]
            .as_str()
            .or_else(|| item["uploader"].as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let video_id = file["video_id"]
            .as_str()
            .or_else(|| item["video_id"].as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let key = normalized_key(&title, uploader.as_deref(), video_id.as_deref());
        let candidate_id = candidate_id(&key);
        if !seen.insert(key) {
            candidates.retain(|candidate| candidate.candidate_id != candidate_id);
        }
        candidates.push(PlaylistCandidate {
            candidate_id,
            title,
            uploader,
            video_id,
            url: url.clone(),
            timestamp: timestamp.to_string(),
            duration: file["duration"]
                .as_f64()
                .or_else(|| item["duration"].as_f64()),
            bytes: file["bytes"].as_u64().unwrap_or(0),
        });
    }
}

fn normalized_key(title: &str, uploader: Option<&str>, video_id: Option<&str>) -> String {
    let title = title.trim().to_ascii_lowercase();
    let uploader = uploader.unwrap_or("").trim().to_ascii_lowercase();
    let video_id = video_id.unwrap_or("").trim().to_ascii_lowercase();
    if video_id.is_empty() {
        format!("{title}\u{1f}{uploader}")
    } else {
        video_id
    }
}

fn candidate_id(key: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("pc_{hex}")
}

fn payload(
    path: std::path::PathBuf,
    skipped_entries: u64,
    candidates: Vec<PlaylistCandidate>,
) -> PlaylistCandidatesPayload {
    PlaylistCandidatesPayload {
        history_path: path.display().to_string(),
        skipped_entries,
        candidates,
    }
}

pub(crate) fn render_playlist_candidates_markdown(payload: &serde_json::Value) -> String {
    let count = payload["candidates"].as_array().map_or(0, Vec::len);
    let mut lines = vec![format!("{count} Plex playlist candidate(s).")];
    for item in payload["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .take(10)
    {
        let title = item["title"].as_str().unwrap_or("Untitled");
        let uploader = item["uploader"].as_str().unwrap_or("Unknown artist");
        lines.push(format!("- {title} - {uploader}"));
    }
    lines.join("\n")
}
