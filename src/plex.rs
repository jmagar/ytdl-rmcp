//! Optional Plex playlist integration for downloaded audio tracks.

mod playlist;

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use url::Url;

use crate::config::Config;

const TRACK_TYPE: &str = "10";

#[allow(unused_imports)]
pub use playlist::{
    apply_audio_tracks, preview_audio_tracks, PlexPlaybackLinks, PlexPlaylistActionResult,
};

#[cfg(test)]
pub(crate) use playlist::{
    apply_audio_tracks_with_transport, playback_links, preview_audio_tracks_with_transport,
};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlexPlaylistUpdate {
    pub playlist: String,
    pub playlist_id: Option<String>,
    pub matched: usize,
    pub added: usize,
    pub already_present: usize,
    pub missing: Vec<PlexMissingTrack>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema, PartialEq)]
pub struct PlexMissingTrack {
    pub title: String,
    pub uploader: Option<String>,
}

/// Plex-owned input DTO: one resolved audio track that the caller wants linked
/// into a playlist. This decouples the Plex integration from the downloader's
/// internal result model — `service.rs` maps `&[ItemResult]` into these before
/// calling [`add_downloaded_audio`], so a change to `downloader::ItemResult`'s
/// shape no longer ripples into this leaf integration.
///
/// Carries only what Plex needs to match a library track: the display `title`
/// and an optional `uploader` (matched against the track/album artist).
#[derive(Debug, Clone)]
pub struct PlexTrackInput {
    pub title: String,
    pub uploader: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TrackCandidate {
    pub(crate) title: String,
    pub(crate) uploader: Option<String>,
}

pub fn add_downloaded_audio(
    cfg: &Config,
    playlist: &str,
    tracks: &[PlexTrackInput],
) -> Result<PlexPlaylistUpdate> {
    apply_audio_tracks(cfg, playlist, tracks).map(PlexPlaylistUpdate::from)
}

#[cfg(test)]
fn add_downloaded_audio_with_transport(
    transport: &mut impl PlexTransport,
    playlist: &str,
    tracks: &[PlexTrackInput],
) -> Result<PlexPlaylistUpdate> {
    apply_audio_tracks_with_transport(transport, playlist, tracks).map(PlexPlaylistUpdate::from)
}

impl From<PlexPlaylistActionResult> for PlexPlaylistUpdate {
    fn from(result: PlexPlaylistActionResult) -> Self {
        Self {
            playlist: result.playlist,
            playlist_id: result.playlist_id,
            matched: result.matched,
            added: result.added,
            already_present: result.already_present,
            missing: result.missing,
            errors: result.errors,
        }
    }
}

/// Tracks whether the target playlist exists yet and which items it already
/// holds, so the add loop can stay a flat match -> skip-if-present -> add.
///
/// The playlist is created lazily: until the first track matches there may be
/// nothing to create the playlist from, so `add_track` creates it on the first
/// added item and appends to it thereafter. `existing` is seeded from the live
/// playlist (when one already exists) and updated as items are added so a
/// duplicate within the same batch is treated as already-present.
pub(crate) struct PlaylistState {
    id: Option<String>,
    existing: BTreeSet<String>,
}

impl PlaylistState {
    pub(crate) fn resolve(transport: &mut impl PlexTransport, playlist: &str) -> Result<Self> {
        let id = find_playlist_id(transport, playlist)?;
        let existing = match id.as_deref() {
            Some(id) => playlist_item_keys(transport, id)?,
            None => BTreeSet::new(),
        };
        Ok(Self { id, existing })
    }

    pub(crate) fn contains(&self, rating_key: &str) -> bool {
        self.existing.contains(rating_key)
    }

    pub(crate) fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Add `rating_key` to the playlist, creating the playlist first if it does
    /// not exist yet. Records the key as present on success.
    pub(crate) fn add_track(
        &mut self,
        transport: &mut impl PlexTransport,
        playlist: &str,
        machine_id: &str,
        rating_key: &str,
    ) -> Result<()> {
        match self.id.as_deref() {
            Some(id) => add_item_to_playlist(transport, id, machine_id, rating_key)?,
            None => {
                let id = create_playlist(transport, playlist, machine_id, rating_key)?;
                self.id = Some(id);
            }
        }
        self.existing.insert(rating_key.to_string());
        Ok(())
    }

    pub(crate) fn into_id(self) -> Option<String> {
        self.id
    }
}

/// Collapse the caller's resolved track inputs into the internal candidate list,
/// dropping empty titles and de-duplicating on a case-insensitive
/// (title, uploader) key while preserving first-seen order.
pub(crate) fn dedup_tracks(tracks: &[PlexTrackInput]) -> Vec<TrackCandidate> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for track in tracks {
        let title = track.title.trim();
        if title.is_empty() {
            continue;
        }
        let uploader = track.uploader.clone();
        let key = format!(
            "{}\u{1f}{}",
            title.to_ascii_lowercase(),
            uploader.as_deref().unwrap_or("").to_ascii_lowercase()
        );
        if seen.insert(key) {
            candidates.push(TrackCandidate {
                title: title.to_string(),
                uploader,
            });
        }
    }
    candidates
}

pub(crate) fn machine_identifier(transport: &mut impl PlexTransport) -> Result<String> {
    let value = transport.get("/identity", &[])?;
    value
        .pointer("/MediaContainer/machineIdentifier")
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("Plex identity response did not include machineIdentifier")
}

fn find_playlist_id(transport: &mut impl PlexTransport, playlist: &str) -> Result<Option<String>> {
    let value = transport.get("/playlists", &[("playlistType", "audio")])?;
    for item in metadata_items(&value) {
        let rating_key = item
            .get("ratingKey")
            .and_then(Value::as_str)
            .map(str::to_string);
        if rating_key.as_deref() == Some(playlist) {
            return Ok(rating_key);
        }
        let title = item.get("title").and_then(Value::as_str);
        if title.is_some_and(|title| title.eq_ignore_ascii_case(playlist)) {
            return Ok(rating_key);
        }
    }
    Ok(None)
}

fn playlist_item_keys(
    transport: &mut impl PlexTransport,
    playlist_id: &str,
) -> Result<BTreeSet<String>> {
    let value = transport.get(&format!("/playlists/{playlist_id}/items"), &[])?;
    Ok(metadata_items(&value)
        .filter_map(|item| item.get("ratingKey").and_then(Value::as_str))
        .map(str::to_string)
        .collect())
}

/// Resolve a downloaded track to a Plex library `ratingKey` via the search API.
///
/// Match criteria, in priority order:
///
/// 1. **Exact match** — a search hit whose `title` matches the candidate title
///    (case-insensitive) AND, when the candidate has an uploader, whose track
///    artist matches that uploader (case-insensitive). Artist is read from
///    `grandparentTitle` (Plex's track/album artist) and `parentTitle` (the
///    album); a hit on either counts. When the candidate has no uploader, the
///    title match alone qualifies. The first exact match wins.
///
///    `originalTitle` is intentionally NOT consulted for artist matching: in
///    Plex it is the track-level *original title* (an alternate name for the
///    track), not an artist field, so matching an uploader against it produces
///    semantically wrong links and false positives.
///
/// 2. **Single-candidate fallback** — if no exact match is found but the search
///    returned exactly one track, return that track. The trade-off: a single
///    hit is very likely the right one (e.g. an artist named slightly
///    differently than the uploader, or a title with extra punctuation), so
///    linking it is usually correct and avoids spurious "missing" reports. When
///    the search returns multiple non-matching candidates we prefer NO match
///    over arbitrarily linking the first one, since picking among ambiguous
///    hits risks attaching the wrong track to the playlist.
///
/// Returns `Ok(None)` when nothing qualifies; the caller records the track as
/// missing rather than guessing.
pub(crate) fn find_track_rating_key(
    transport: &mut impl PlexTransport,
    track: &TrackCandidate,
) -> Result<Option<String>> {
    let value = transport.get(
        "/search",
        &[("query", track.title.as_str()), ("type", TRACK_TYPE)],
    )?;
    let mut sole_candidate: Option<String> = None;
    let mut candidate_count = 0usize;
    for item in metadata_items(&value) {
        let Some(rating_key) = item.get("ratingKey").and_then(Value::as_str) else {
            continue;
        };
        candidate_count += 1;
        if candidate_count == 1 {
            sole_candidate = Some(rating_key.to_string());
        }
        let title_matches = item
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|title| title.eq_ignore_ascii_case(&track.title));
        let uploader_matches = match track.uploader.as_deref() {
            Some(uploader) => ["grandparentTitle", "parentTitle"]
                .iter()
                .filter_map(|field| item.get(*field).and_then(Value::as_str))
                .any(|artist| artist.eq_ignore_ascii_case(uploader)),
            None => true,
        };
        if title_matches && uploader_matches {
            return Ok(Some(rating_key.to_string()));
        }
    }
    // No exact match: only fall back when the result set is unambiguous (a
    // single candidate). Otherwise prefer no match over a possibly-wrong link.
    if candidate_count == 1 {
        Ok(sole_candidate)
    } else {
        Ok(None)
    }
}

fn create_playlist(
    transport: &mut impl PlexTransport,
    playlist: &str,
    machine_id: &str,
    rating_key: &str,
) -> Result<String> {
    let uri = library_uri(machine_id, rating_key);
    let value = transport.post(
        "/playlists",
        &[
            ("type", "audio"),
            ("title", playlist),
            ("smart", "0"),
            ("uri", uri.as_str()),
        ],
    )?;
    let rating_key = metadata_items(&value)
        .find_map(|item| item.get("ratingKey").and_then(Value::as_str))
        .map(str::to_string)
        .context("Plex create playlist response did not include ratingKey")?;
    Ok(rating_key)
}

fn add_item_to_playlist(
    transport: &mut impl PlexTransport,
    playlist_id: &str,
    machine_id: &str,
    rating_key: &str,
) -> Result<()> {
    let uri = library_uri(machine_id, rating_key);
    transport.put(
        &format!("/playlists/{playlist_id}/items"),
        &[("uri", uri.as_str())],
    )
}

fn library_uri(machine_id: &str, rating_key: &str) -> String {
    format!("server://{machine_id}/com.plexapp.plugins.library/library/metadata/{rating_key}")
}

fn metadata_items(value: &Value) -> impl Iterator<Item = &Value> {
    value
        .pointer("/MediaContainer/Metadata")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// Build a token-free display form of a Plex request URL for error/log use.
///
/// The live request URL carries the `X-Plex-Token` query parameter (the Plex
/// auth secret). That value must never reach an error chain or the tracing
/// output, because Plex errors are surfaced to the MCP client (as
/// `plex_playlist_error`) and logged to stderr. This rebuilds the URL with the
/// token value replaced by `REDACTED` while leaving the path and the
/// non-sensitive query params intact for debugging.
fn redact_url(url: &Url) -> String {
    let mut redacted = url.clone();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            if key == "X-Plex-Token" {
                (key.into_owned(), "REDACTED".to_string())
            } else {
                (key.into_owned(), value.into_owned())
            }
        })
        .collect();
    {
        let mut serializer = redacted.query_pairs_mut();
        serializer.clear();
        for (key, value) in &pairs {
            serializer.append_pair(key, value);
        }
    }
    if pairs.is_empty() {
        redacted.set_query(None);
    }
    redacted.to_string()
}

pub(crate) trait PlexTransport {
    fn get(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value>;
    fn post(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value>;
    fn put(&mut self, path: &str, params: &[(&str, &str)]) -> Result<()>;
}

pub(crate) struct UreqPlexTransport {
    base_url: String,
    token: String,
}

pub(crate) fn transport_from_config(cfg: &Config) -> Result<UreqPlexTransport> {
    let base_url = cfg
        .plex_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .context("YTDLP_PLEX_URL is required for Plex playlist actions")?;
    let token = cfg
        .plex_token
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .context("YTDLP_PLEX_TOKEN is required for Plex playlist actions")?;
    Ok(UreqPlexTransport {
        base_url: base_url.to_string(),
        token: token.to_string(),
    })
}

impl UreqPlexTransport {
    fn url(&self, path: &str, params: &[(&str, &str)]) -> Result<Url> {
        let base = format!("{}/", self.base_url.trim_end_matches('/'));
        let mut url = Url::parse(&base).context("parse YTDLP_PLEX_URL")?;
        url.set_path(path.trim_start_matches('/'));
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("X-Plex-Token", &self.token);
            for (key, value) in params {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    fn read_json(&self, mut response: ureq::http::Response<ureq::Body>) -> Result<Value> {
        if !response.status().is_success() {
            bail!("Plex returned HTTP {}", response.status());
        }
        let mut reader = response.body_mut().as_reader();
        serde_json::from_reader(&mut reader).context("parse Plex JSON response")
    }
}

impl PlexTransport for UreqPlexTransport {
    fn get(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let url = self.url(path, params)?;
        let response = ureq::get(url.as_str())
            .header("Accept", "application/json")
            .call()
            .with_context(|| format!("GET {}", redact_url(&url)))?;
        self.read_json(response)
    }

    fn post(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let url = self.url(path, params)?;
        let response = ureq::post(url.as_str())
            .header("Accept", "application/json")
            .send_empty()
            .with_context(|| format!("POST {}", redact_url(&url)))?;
        self.read_json(response)
    }

    fn put(&mut self, path: &str, params: &[(&str, &str)]) -> Result<()> {
        let url = self.url(path, params)?;
        let response = ureq::put(url.as_str())
            .header("Accept", "application/json")
            .send_empty()
            .with_context(|| format!("PUT {}", redact_url(&url)))?;
        if !response.status().is_success() {
            bail!("Plex returned HTTP {}", response.status());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "plex_tests.rs"]
mod tests;
