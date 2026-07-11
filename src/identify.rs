use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use tokio::process::Command;

use crate::util::{json_str, run_capped};

mod musicbrainz;
mod tagger;
pub(crate) use musicbrainz::{MusicBrainzLookup, RetagPreview, UreqMusicBrainzLookup};
pub(crate) use tagger::TagWriteResult;

const RETAG_PREVIEW_MIN_SCORE: f64 = 0.90;

/// PerfL3: HTTP timeout budget shared by every identify network call. Both the
/// AcoustID and MusicBrainz lookups run through a single pooled [`ureq::Agent`]
/// built with this global + connect timeout, so a stuck endpoint can't hang the
/// blocking task indefinitely and connections are reused across the batch.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the shared `ureq::Agent` used by the AcoustID + MusicBrainz lookups
/// (connection pooling + an explicit timeout budget).
pub(crate) fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .timeout_connect(Some(HTTP_TIMEOUT))
        .build()
        .into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub duration: u32,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IdentifyCandidate {
    pub acoustid_id: String,
    pub score: f64,
    pub recording_id: Option<String>,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub release_group: Option<String>,
    pub release_group_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IdentifyResult {
    pub path: String,
    pub duration: Option<u32>,
    pub candidates: Vec<IdentifyCandidate>,
    pub retag_preview: Option<RetagPreview>,
    pub retag_preview_error: Option<String>,
    pub tag_write: Option<TagWriteResult>,
    pub tag_write_error: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IdentifyPayload {
    pub results: Vec<IdentifyResult>,
}

pub(crate) trait AcoustIdLookup: Send {
    fn lookup(&mut self, fingerprint: &Fingerprint) -> Result<Vec<IdentifyCandidate>>;
}

pub async fn identify_files(
    cfg: &crate::config::Config,
    paths: Vec<String>,
    write_tags: bool,
) -> Result<IdentifyPayload> {
    let client_key = cfg
        .acoustid_client_key
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .context("YTDLP_ACOUSTID_CLIENT_KEY is required for youtube_identify")?
        .to_string();
    let fpcalc = resolve_fpcalc(cfg)?;
    let user_agent = user_agent(cfg);
    let timeout = Some(cfg.ytdlp_timeout());

    // Phase 1 (async): fingerprint every file via the fpcalc subprocess on the
    // tokio runtime, where `run_capped` belongs. Pair each path with its
    // fingerprint result so the blocking phase can short-circuit failures.
    let mut prepared: Vec<(PathBuf, Result<Fingerprint>)> = Vec::with_capacity(paths.len());
    for path in paths {
        let path = PathBuf::from(path);
        let fingerprint = fingerprint_file(&fpcalc, &path, timeout).await;
        prepared.push((path, fingerprint));
    }

    // Phase 2 (spawn_blocking): the AcoustID + MusicBrainz lookups use blocking
    // `ureq`, and the MusicBrainz rate-limiter sleeps with `std::thread::sleep`.
    // Offload the whole loop (C1/TH1) so no blocking HTTP or sleep ever runs on
    // a reactor worker thread. Tag writing (blocking std::fs) rides along here.
    let results = tokio::task::spawn_blocking(move || {
        let mut lookup = UreqAcoustIdLookup::new(client_key, user_agent.clone());
        let mut musicbrainz = UreqMusicBrainzLookup::new(user_agent);
        prepared
            .into_iter()
            .map(|(path, fingerprint)| {
                identify_from_fingerprint(
                    &path,
                    fingerprint,
                    &mut lookup,
                    Some(&mut musicbrainz),
                    write_tags,
                )
            })
            .collect::<Vec<_>>()
    })
    .await?;

    Ok(IdentifyPayload { results })
}

/// Synchronous post-fingerprint pipeline for a single file: AcoustID lookup →
/// optional MusicBrainz retag preview → optional tag write. Runs inside
/// `spawn_blocking` (all blocking I/O). `fingerprint` is the already-computed
/// fpcalc result so this function never touches the async runtime.
fn identify_from_fingerprint(
    path: &Path,
    fingerprint: Result<Fingerprint>,
    lookup: &mut dyn AcoustIdLookup,
    musicbrainz: Option<&mut dyn MusicBrainzLookup>,
    write_tags: bool,
) -> IdentifyResult {
    let mut result = IdentifyResult {
        path: path.display().to_string(),
        duration: None,
        candidates: Vec::new(),
        retag_preview: None,
        retag_preview_error: None,
        tag_write: None,
        tag_write_error: None,
        error: None,
    };

    let fingerprint = match fingerprint {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            result.error = Some(error.to_string());
            return result;
        }
    };
    result.duration = Some(fingerprint.duration);

    match lookup.lookup(&fingerprint) {
        Ok(candidates) => result.candidates = candidates,
        Err(error) => result.error = Some(error.to_string()),
    }
    if let Some(musicbrainz) = musicbrainz {
        add_retag_preview(&mut result, musicbrainz);
    }
    if write_tags {
        write_retag_preview(&mut result, path);
    }
    result
}

fn resolve_fpcalc(cfg: &crate::config::Config) -> Result<PathBuf> {
    if let Some(path) = cfg.fpcalc_path.as_deref().filter(|s| !s.trim().is_empty()) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        bail!("FPCALC_PATH does not exist: {}", path.display());
    }
    which::which("fpcalc").context("fpcalc was not found on PATH; set FPCALC_PATH")
}

fn user_agent(cfg: &crate::config::Config) -> String {
    let version = env!("CARGO_PKG_VERSION");
    match cfg
        .musicbrainz_contact
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        Some(contact) => format!("ytdl-rmcp/{version} ({contact})"),
        None => format!("ytdl-rmcp/{version} (https://github.com/jmagar/ytdl-rmcp)"),
    }
}

// Test-only convenience seam: the production path goes through `identify_files`
// (fingerprint phase, then a `spawn_blocking` lookup phase). Tests drive a single
// file synchronously against fake lookups.
#[cfg(test)]
pub(crate) async fn identify_file_with_client(
    fpcalc: &Path,
    path: &Path,
    timeout: Option<Duration>,
    lookup: &mut dyn AcoustIdLookup,
    musicbrainz: Option<&mut dyn MusicBrainzLookup>,
    write_tags: bool,
) -> IdentifyResult {
    let fingerprint = fingerprint_file(fpcalc, path, timeout).await;
    identify_from_fingerprint(path, fingerprint, lookup, musicbrainz, write_tags)
}

fn add_retag_preview(result: &mut IdentifyResult, musicbrainz: &mut dyn MusicBrainzLookup) {
    let Some(candidate) = result
        .candidates
        .iter()
        .filter(|candidate| candidate.score >= RETAG_PREVIEW_MIN_SCORE)
        .max_by(|a, b| a.score.total_cmp(&b.score))
    else {
        return;
    };
    let Some(recording_id) = candidate.recording_id.as_deref() else {
        return;
    };
    match musicbrainz.lookup_recording(recording_id, candidate.score) {
        Ok(preview) => result.retag_preview = Some(preview),
        Err(error) => result.retag_preview_error = Some(error.to_string()),
    }
}

fn write_retag_preview(result: &mut IdentifyResult, path: &Path) {
    let Some(preview) = result.retag_preview.as_ref() else {
        return;
    };
    match tagger::write_retag_preview(path, preview) {
        Ok(write) => result.tag_write = Some(write),
        Err(error) => result.tag_write_error = Some(error.to_string()),
    }
}

/// Bound fpcalc's stderr to the last 16 KiB; a misbehaving fpcalc could
/// otherwise stream unbounded diagnostics into memory.
const FPCALC_STDERR_TAIL_BYTES: usize = 16 * 1024;

async fn fingerprint_file(
    fpcalc: &Path,
    path: &Path,
    timeout: Option<Duration>,
) -> Result<Fingerprint> {
    let mut cmd = Command::new(fpcalc);
    // end-of-options: prevent a '-'-prefixed path from being parsed as an fpcalc
    // flag (e.g. -length, -algorithm) instead of the file to fingerprint (F2).
    cmd.arg("--").arg(path);
    let output = run_capped(&mut cmd, timeout, Some(FPCALC_STDERR_TAIL_BYTES)).await?;
    let parsed = parse_fpcalc_output(&output.stdout);
    if !output.status.success() && parsed.is_err() {
        bail!("{}", output.stderr.trim());
    }
    parsed
}

pub(crate) fn parse_fpcalc_output(bytes: &[u8]) -> Result<Fingerprint> {
    let text = String::from_utf8_lossy(bytes);
    let mut duration = None;
    let mut fingerprint = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "DURATION" => duration = value.trim().parse().ok(),
            "FINGERPRINT" => fingerprint = non_empty(value),
            _ => {}
        }
    }
    let Some(duration) = duration else {
        bail!("fpcalc output did not include DURATION");
    };
    let Some(fingerprint) = fingerprint else {
        bail!("fpcalc output did not include FINGERPRINT");
    };
    Ok(Fingerprint {
        duration,
        fingerprint,
    })
}

pub(crate) fn parse_acoustid_lookup(bytes: &[u8]) -> Result<Vec<IdentifyCandidate>> {
    let value: Value = serde_json::from_slice(bytes)?;
    if value["status"].as_str() != Some("ok") {
        bail!(
            "AcoustID lookup failed: {}",
            value["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
        );
    }

    let candidates = value["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(result_candidates)
        .collect();
    Ok(candidates)
}

fn result_candidates(result: &Value) -> Vec<IdentifyCandidate> {
    let acoustid_id = json_str(result, "id").unwrap_or_default();
    let score = result["score"].as_f64().unwrap_or(0.0);
    result["recordings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|recording| IdentifyCandidate {
            acoustid_id: acoustid_id.clone(),
            score,
            recording_id: json_str(recording, "id"),
            title: json_str(recording, "title"),
            artists: recording["artists"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|artist| json_str(artist, "name"))
                .collect(),
            release_group: recording["releasegroups"]
                .as_array()
                .and_then(|groups| groups.first())
                .and_then(|group| json_str(group, "title")),
            release_group_type: recording["releasegroups"]
                .as_array()
                .and_then(|groups| groups.first())
                .and_then(|group| json_str(group, "type")),
        })
        .collect()
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

struct UreqAcoustIdLookup {
    client_key: String,
    user_agent: String,
    agent: ureq::Agent,
    /// PerfM4: memoize AcoustID results within a batch, keyed by the fingerprint
    /// string. Duplicate downloads (same audio) then cost a single network call.
    cache: HashMap<String, Vec<IdentifyCandidate>>,
}

impl UreqAcoustIdLookup {
    fn new(client_key: String, user_agent: String) -> Self {
        Self {
            client_key,
            user_agent,
            agent: http_agent(),
            cache: HashMap::new(),
        }
    }
}

impl AcoustIdLookup for UreqAcoustIdLookup {
    fn lookup(&mut self, fingerprint: &Fingerprint) -> Result<Vec<IdentifyCandidate>> {
        if let Some(candidates) = self.cache.get(&fingerprint.fingerprint) {
            return Ok(candidates.clone());
        }
        let mut response = self
            .agent
            .post("https://api.acoustid.org/v2/lookup")
            .header("Accept", "application/json")
            .header("User-Agent", &self.user_agent)
            .send_form([
                ("format", "json"),
                ("client", self.client_key.as_str()),
                ("duration", &fingerprint.duration.to_string()),
                ("fingerprint", fingerprint.fingerprint.as_str()),
                ("meta", "recordings releasegroups compress"),
            ])
            .context("call AcoustID lookup")?;
        if !response.status().is_success() {
            bail!("AcoustID returned HTTP {}", response.status());
        }
        let bytes = response
            .body_mut()
            .read_to_vec()
            .context("read AcoustID response")?;
        let candidates = parse_acoustid_lookup(&bytes)?;
        self.cache
            .insert(fingerprint.fingerprint.clone(), candidates.clone());
        Ok(candidates)
    }
}

pub fn render_identify_markdown(payload: &Value) -> String {
    let mut lines = Vec::new();
    for result in payload["results"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{}:",
            result["path"].as_str().unwrap_or("unknown path")
        ));
        if let Some(error) = result["error"].as_str() {
            lines.push(format!("- error: {error}"));
            continue;
        }
        if let Some(duration) = result["duration"].as_u64() {
            lines.push(format!("- duration: {duration}s"));
        }
        let candidates = result["candidates"].as_array().cloned().unwrap_or_default();
        if candidates.is_empty() {
            lines.push("- no candidates".into());
            continue;
        }
        for candidate in candidates.iter().take(5) {
            let title = candidate["title"].as_str().unwrap_or("Untitled");
            let artists = candidate["artists"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            let artists = if artists.is_empty() {
                "Unknown Artist".to_string()
            } else {
                artists.join(", ")
            };
            let recording = candidate["recording_id"]
                .as_str()
                .unwrap_or("no recording id");
            lines.push(format!(
                "- {:.0}% - {artists} - {title} ({recording})",
                candidate["score"].as_f64().unwrap_or(0.0) * 100.0
            ));
        }
        if let Some(preview) = result["retag_preview"].as_object() {
            let artist = preview
                .get("artist")
                .and_then(Value::as_str)
                .unwrap_or("Unknown Artist");
            let title = preview
                .get("recording_title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled");
            lines.push(format!("- would retag as: {artist} - {title}"));
            if let Some(album) = preview.get("release_title").and_then(Value::as_str) {
                lines.push(format!("  album: {album}"));
            }
            if let Some(date) = preview.get("release_date").and_then(Value::as_str) {
                lines.push(format!("  date: {date}"));
            }
            if let Some(track) = preview.get("track_number").and_then(Value::as_str) {
                lines.push(format!("  track: {track}"));
            }
        }
        if let Some(write) = result["tag_write"].as_object() {
            let fields = write["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            if fields.is_empty() {
                lines.push("- tags written".into());
            } else {
                lines.push(format!("- tags written: {}", fields.join(", ")));
            }
        }
        if let Some(error) = result["tag_write_error"].as_str() {
            lines.push(format!("- tag write failed: {error}"));
        }
        if let Some(error) = result["retag_preview_error"].as_str() {
            lines.push(format!("- retag preview unavailable: {error}"));
        }
    }
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
#[path = "identify_tests.rs"]
mod tests;
