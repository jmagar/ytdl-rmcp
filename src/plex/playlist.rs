use anyhow::Result;
use serde::Serialize;

use super::{
    dedup_tracks, find_track_rating_key, machine_identifier, PlaylistState, PlexMissingTrack,
    PlexTrackInput, PlexTransport, TrackCandidate,
};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, schemars::JsonSchema, PartialEq)]
pub struct PlexPlaylistActionResult {
    pub playlist: String,
    pub playlist_id: Option<String>,
    pub machine_identifier: Option<String>,
    pub matched: usize,
    pub added: usize,
    pub already_present: usize,
    pub missing: Vec<PlexMissingTrack>,
    pub errors: Vec<String>,
    pub plexamp_url: Option<String>,
    pub plex_web_url: Option<String>,
    pub playback_link_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema, PartialEq)]
pub struct PlexPlaybackLinks {
    pub plexamp_url: String,
    pub plex_web_url: String,
    pub playback_link_status: String,
}

pub fn preview_audio_tracks(
    cfg: &Config,
    playlist: &str,
    tracks: &[PlexTrackInput],
) -> Result<PlexPlaylistActionResult> {
    let tracks = dedup_tracks(tracks);
    if tracks.is_empty() {
        return Ok(empty_result(playlist));
    }
    let mut transport = super::transport_from_config(cfg)?;
    run_playlist_plan(&mut transport, playlist, tracks, false)
}

pub fn apply_audio_tracks(
    cfg: &Config,
    playlist: &str,
    tracks: &[PlexTrackInput],
) -> Result<PlexPlaylistActionResult> {
    let tracks = dedup_tracks(tracks);
    if tracks.is_empty() {
        return Ok(empty_result(playlist));
    }
    let mut transport = super::transport_from_config(cfg)?;
    run_playlist_plan(&mut transport, playlist, tracks, true)
}

#[cfg(test)]
pub(crate) fn preview_audio_tracks_with_transport(
    transport: &mut impl PlexTransport,
    playlist: &str,
    tracks: &[PlexTrackInput],
) -> Result<PlexPlaylistActionResult> {
    run_playlist_plan(transport, playlist, dedup_tracks(tracks), false)
}

#[cfg(test)]
pub(crate) fn apply_audio_tracks_with_transport(
    transport: &mut impl PlexTransport,
    playlist: &str,
    tracks: &[PlexTrackInput],
) -> Result<PlexPlaylistActionResult> {
    run_playlist_plan(transport, playlist, dedup_tracks(tracks), true)
}

fn run_playlist_plan(
    transport: &mut impl PlexTransport,
    playlist: &str,
    tracks: Vec<TrackCandidate>,
    mutate: bool,
) -> Result<PlexPlaylistActionResult> {
    let mut result = empty_result(playlist);

    if tracks.is_empty() {
        return Ok(result);
    }

    let machine_id = machine_identifier(transport)?;
    let mut state = PlaylistState::resolve(transport, playlist)?;
    result.playlist_id = state.id().map(str::to_string);
    result.machine_identifier = Some(machine_id.clone());

    for track in tracks {
        let rating_key = match find_track_rating_key(transport, &track) {
            Ok(Some(rating_key)) => rating_key,
            Ok(None) => {
                result.missing.push(PlexMissingTrack {
                    title: track.title,
                    uploader: track.uploader,
                });
                continue;
            }
            Err(error) => {
                result.errors.push(format!("{}: {error}", track.title));
                continue;
            }
        };
        result.matched += 1;
        if state.contains(&rating_key) {
            result.already_present += 1;
            continue;
        }
        if mutate {
            if let Err(error) = state.add_track(transport, playlist, &machine_id, &rating_key) {
                result.errors.push(format!("{}: {error}", track.title));
                continue;
            }
            result.added += 1;
        }
    }

    result.playlist_id = state.into_id();
    if let Some(playlist_id) = result.playlist_id.as_deref() {
        let links = playback_links(&machine_id, playlist_id);
        result.plexamp_url = Some(links.plexamp_url);
        result.plex_web_url = Some(links.plex_web_url);
        result.playback_link_status = Some(links.playback_link_status);
    }
    Ok(result)
}

fn empty_result(playlist: &str) -> PlexPlaylistActionResult {
    PlexPlaylistActionResult {
        playlist: playlist.to_string(),
        playlist_id: None,
        machine_identifier: None,
        matched: 0,
        added: 0,
        already_present: 0,
        missing: Vec::new(),
        errors: Vec::new(),
        plexamp_url: None,
        plex_web_url: None,
        playback_link_status: None,
    }
}

pub fn playback_links(machine_id: &str, playlist_id: &str) -> PlexPlaybackLinks {
    let server_uri =
        format!("server://{machine_id}/com.plexapp.plugins.library/playlists/{playlist_id}/items");
    PlexPlaybackLinks {
        plexamp_url: format!(
            "https://listen.plex.tv/player/playback/playMedia?uri={}",
            encode_component(&server_uri)
        ),
        plex_web_url: format!(
            "https://app.plex.tv/desktop/#!/server/{}/playlist?key={}",
            encode_component(machine_id),
            encode_component(&format!("/playlists/{playlist_id}/items"))
        ),
        playback_link_status: "generated_unverified".to_string(),
    }
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
