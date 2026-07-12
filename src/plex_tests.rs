use super::*;
use serde_json::json;

#[derive(Default)]
struct FakePlex {
    gets: Vec<(String, Vec<(String, String)>)>,
    posts: Vec<(String, Vec<(String, String)>)>,
    puts: Vec<(String, Vec<(String, String)>)>,
}

impl PlexTransport for FakePlex {
    fn get(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.gets.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        match path {
            "/identity" => Ok(json!({
                "MediaContainer": { "machineIdentifier": "machine-1" }
            })),
            "/playlists" => Ok(json!({
                "MediaContainer": {
                    "Metadata": [
                        { "ratingKey": "99", "title": "Downloads" }
                    ]
                }
            })),
            "/playlists/99/items" => Ok(json!({
                "MediaContainer": {
                    "Metadata": [
                        { "ratingKey": "111", "title": "Already There" }
                    ]
                }
            })),
            "/search" => {
                let query = params
                    .iter()
                    .find(|(key, _)| *key == "query")
                    .map(|(_, value)| *value)
                    .unwrap_or("");
                if query == "Already There" {
                    Ok(json!({
                        "MediaContainer": {
                            "Metadata": [
                                { "ratingKey": "111", "type": "track", "title": "Already There", "grandparentTitle": "Artist A" }
                            ]
                        }
                    }))
                } else if query == "New Song" {
                    Ok(json!({
                        "MediaContainer": {
                            "Metadata": [
                                { "ratingKey": "222", "type": "track", "title": "New Song", "grandparentTitle": "Artist B" }
                            ]
                        }
                    }))
                } else {
                    Ok(json!({ "MediaContainer": { "Metadata": [] } }))
                }
            }
            _ => bail!("unexpected GET {path}"),
        }
    }

    fn post(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.posts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(json!({
            "MediaContainer": {
                "Metadata": [
                    { "ratingKey": "100", "title": "Downloads" }
                ]
            }
        }))
    }

    fn put(&mut self, path: &str, params: &[(&str, &str)]) -> Result<()> {
        self.puts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(())
    }
}

#[test]
fn add_downloaded_audio_skips_existing_and_adds_missing_playlist_items() {
    let mut plex = FakePlex::default();
    let tracks = vec![
        track("Already There", "Artist A"),
        track("New Song", "Artist B"),
    ];

    let update =
        add_downloaded_audio_with_transport(&mut plex, "Downloads", &tracks).expect("plex update");

    assert_eq!(update.playlist, "Downloads");
    assert_eq!(update.playlist_id.as_deref(), Some("99"));
    assert_eq!(update.matched, 2);
    assert_eq!(update.added, 1);
    assert_eq!(update.already_present, 1);
    assert!(update.missing.is_empty());
    assert_eq!(plex.posts.len(), 0);
    assert_eq!(plex.puts.len(), 1);
    assert_eq!(plex.puts[0].0, "/playlists/99/items");
    assert_eq!(
        plex.puts[0].1[0].1,
        "server://machine-1/com.plexapp.plugins.library/library/metadata/222"
    );
}

#[test]
fn add_downloaded_audio_creates_playlist_with_first_matched_track() {
    let mut plex = CreatePlaylistPlex::default();
    let tracks = vec![track("New Song", "Artist B")];

    let update =
        add_downloaded_audio_with_transport(&mut plex, "Fresh List", &tracks).expect("plex update");

    assert_eq!(update.playlist_id.as_deref(), Some("100"));
    assert_eq!(update.matched, 1);
    assert_eq!(update.added, 1);
    assert_eq!(plex.posts.len(), 1);
    assert_eq!(plex.posts[0].0, "/playlists");
    assert!(plex.puts.is_empty());
}

#[test]
fn preview_playlist_does_not_mutate_plex() {
    let mut plex = FakePlex::default();
    let tracks = vec![track("New Song", "Artist B")];

    let result = preview_audio_tracks_with_transport(&mut plex, "Downloads", &tracks).unwrap();

    assert_eq!(result.matched, 1);
    assert_eq!(result.added, 0);
    assert_eq!(result.already_present, 0);
    assert!(plex.posts.is_empty());
    assert!(plex.puts.is_empty());
}

#[test]
fn apply_playlist_returns_best_effort_plexamp_link() {
    let mut plex = FakePlex::default();
    let tracks = vec![track("New Song", "Artist B")];

    let result = apply_audio_tracks_with_transport(&mut plex, "Downloads", &tracks).unwrap();

    assert_eq!(result.playlist_id.as_deref(), Some("99"));
    assert_eq!(
        result.playback_link_status.as_deref(),
        Some("generated_unverified")
    );
    assert!(result
        .plexamp_url
        .as_deref()
        .unwrap()
        .starts_with("https://listen.plex.tv/player/playback/playMedia?uri="));
    assert!(!result.plexamp_url.unwrap().contains("X-Plex-Token"));
}

#[test]
fn add_downloaded_audio_without_audio_files_does_not_require_plex_config() {
    let cfg = Config {
        target_path: None,
        video_target_path: None,
        allow_local_targets: false,
        staging_dir: None,
        audio_format: "mp3".into(),
        ssh_opts: Vec::new(),
        archive_dir: None,
        history_path: None,
        plex_url: None,
        plex_token: None,
        plex_playlist: None,
        clean_metadata: true,
        acoustid_client_key: None,
        fpcalc_path: None,
        musicbrainz_contact: None,
        auto_update: false,
        max_age_days: 14,
        update_pre: false,
        ytdlp_path: None,
        ffmpeg_path: None,
        extractor_args: None,
        ytdlp_sha256: None,
        ffmpeg_sha256: None,
        ytdlp_timeout_secs: 5,
        transfer_timeout_secs: 5,
    };
    // A video-only download yields no Plex audio inputs, so the caller passes an
    // empty slice and Plex config is never required.
    let tracks: Vec<PlexTrackInput> = Vec::new();

    let update = add_downloaded_audio(&cfg, "Downloads", &tracks).expect("empty update");

    assert_eq!(update.playlist, "Downloads");
    assert_eq!(update.matched, 0);
    assert_eq!(update.added, 0);
    assert_eq!(update.already_present, 0);
}

#[derive(Default)]
struct CreatePlaylistPlex {
    posts: Vec<(String, Vec<(String, String)>)>,
    puts: Vec<(String, Vec<(String, String)>)>,
}

impl PlexTransport for CreatePlaylistPlex {
    fn get(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        match path {
            "/identity" => Ok(json!({
                "MediaContainer": { "machineIdentifier": "machine-1" }
            })),
            "/playlists" => Ok(json!({ "MediaContainer": { "Metadata": [] } })),
            "/search" => {
                let query = params
                    .iter()
                    .find(|(key, _)| *key == "query")
                    .map(|(_, value)| *value)
                    .unwrap_or("");
                assert_eq!(query, "New Song");
                Ok(json!({
                    "MediaContainer": {
                        "Metadata": [
                            { "ratingKey": "222", "type": "track", "title": "New Song", "grandparentTitle": "Artist B" }
                        ]
                    }
                }))
            }
            _ => bail!("unexpected GET {path}"),
        }
    }

    fn post(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.posts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(json!({
            "MediaContainer": {
                "Metadata": [
                    { "ratingKey": "100", "title": "Fresh List" }
                ]
            }
        }))
    }

    fn put(&mut self, path: &str, params: &[(&str, &str)]) -> Result<()> {
        self.puts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(())
    }
}

/// Fake whose `/search` returns a single near-miss: the title differs from the
/// query, and no artist field matches the uploader. Used to exercise the
/// fallback logic in `find_track_rating_key`.
#[derive(Default)]
struct NearMissPlex {
    posts: Vec<(String, Vec<(String, String)>)>,
    puts: Vec<(String, Vec<(String, String)>)>,
    // Extra non-matching candidates to append to the single near-miss hit.
    extra_candidates: Vec<Value>,
}

impl PlexTransport for NearMissPlex {
    fn get(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        match path {
            "/identity" => Ok(json!({
                "MediaContainer": { "machineIdentifier": "machine-1" }
            })),
            "/playlists" => Ok(json!({ "MediaContainer": { "Metadata": [] } })),
            "/search" => {
                let query = params
                    .iter()
                    .find(|(key, _)| *key == "query")
                    .map(|(_, value)| *value)
                    .unwrap_or("");
                let mut metadata = vec![json!({
                    "ratingKey": "555",
                    "type": "track",
                    // Title intentionally NOT equal to the query.
                    "title": format!("{query} (Live)"),
                    "grandparentTitle": "Some Other Artist"
                })];
                metadata.extend(self.extra_candidates.iter().cloned());
                Ok(json!({ "MediaContainer": { "Metadata": metadata } }))
            }
            _ => bail!("unexpected GET {path}"),
        }
    }

    fn post(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.posts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(json!({
            "MediaContainer": { "Metadata": [ { "ratingKey": "777", "title": "Fallback List" } ] }
        }))
    }

    fn put(&mut self, path: &str, params: &[(&str, &str)]) -> Result<()> {
        self.puts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(())
    }
}

#[test]
fn single_near_miss_candidate_falls_back_to_sole_hit() {
    // One non-exact search hit -> unambiguous -> fall back to it (matched).
    let mut plex = NearMissPlex::default();
    let tracks = vec![track("Mystery Track", "Different Uploader")];

    let update = add_downloaded_audio_with_transport(&mut plex, "Fallback List", &tracks)
        .expect("plex update");

    assert_eq!(update.matched, 1);
    assert_eq!(update.added, 1);
    assert!(update.missing.is_empty());
    // Playlist did not exist, so the sole-candidate fallback should have
    // created it from rating key 555.
    assert_eq!(plex.posts.len(), 1);
    assert_eq!(
        plex.posts[0]
            .1
            .iter()
            .find(|(k, _)| k == "uri")
            .map(|(_, v)| v.as_str()),
        Some("server://machine-1/com.plexapp.plugins.library/library/metadata/555")
    );
}

#[test]
fn ambiguous_near_miss_candidates_record_missing() {
    // Two non-exact candidates -> ambiguous -> prefer no match over a guess.
    let mut plex = NearMissPlex {
        extra_candidates: vec![json!({
            "ratingKey": "556",
            "type": "track",
            "title": "Mystery Track (Remix)",
            "grandparentTitle": "Yet Another Artist"
        })],
        ..Default::default()
    };
    let tracks = vec![track("Mystery Track", "Different Uploader")];

    let update = add_downloaded_audio_with_transport(&mut plex, "Fallback List", &tracks)
        .expect("plex update");

    assert_eq!(update.matched, 0);
    assert_eq!(update.added, 0);
    assert_eq!(update.missing.len(), 1);
    assert_eq!(update.missing[0].title, "Mystery Track");
    assert!(plex.posts.is_empty());
}

/// Fake whose `/search` hit matches the uploader only via `parentTitle` (the
/// album), confirming `parentTitle` participates in artist matching.
#[derive(Default)]
struct ParentTitleMatchPlex {
    posts: Vec<(String, Vec<(String, String)>)>,
    puts: Vec<(String, Vec<(String, String)>)>,
}

impl PlexTransport for ParentTitleMatchPlex {
    fn get(&mut self, path: &str, _params: &[(&str, &str)]) -> Result<Value> {
        match path {
            "/identity" => Ok(json!({
                "MediaContainer": { "machineIdentifier": "machine-1" }
            })),
            "/playlists" => Ok(json!({ "MediaContainer": { "Metadata": [] } })),
            "/search" => Ok(json!({
                "MediaContainer": {
                    "Metadata": [
                        {
                            "ratingKey": "333",
                            "type": "track",
                            "title": "Album Cut",
                            // Artist field does NOT match; album (parentTitle) does.
                            "grandparentTitle": "Unrelated Artist",
                            "parentTitle": "Matching Uploader",
                            // originalTitle must be ignored even though it would match.
                            "originalTitle": "Should Be Ignored"
                        }
                    ]
                }
            })),
            _ => bail!("unexpected GET {path}"),
        }
    }

    fn post(&mut self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        self.posts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(json!({
            "MediaContainer": { "Metadata": [ { "ratingKey": "333", "title": "Album List" } ] }
        }))
    }

    fn put(&mut self, path: &str, params: &[(&str, &str)]) -> Result<()> {
        self.puts.push((
            path.to_string(),
            params
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        Ok(())
    }
}

#[test]
fn parent_title_satisfies_artist_match() {
    let mut plex = ParentTitleMatchPlex::default();
    let tracks = vec![track("Album Cut", "Matching Uploader")];

    let update =
        add_downloaded_audio_with_transport(&mut plex, "Album List", &tracks).expect("plex update");

    assert_eq!(update.matched, 1);
    assert_eq!(update.added, 1);
    assert!(update.missing.is_empty());
}

#[test]
fn playback_links_use_path_component_percent_encoding() {
    let links = playback_links("machine id", "playlist 1");

    assert!(links.plexamp_url.contains("machine%20id"));
    assert!(links.plexamp_url.contains("playlist%201"));
    assert!(!links.plexamp_url.contains('+'));
    assert!(links.plex_web_url.contains("machine%20id"));
    assert!(links
        .plex_web_url
        .contains("%2Fplaylists%2Fplaylist%201%2Fitems"));
}

#[test]
fn redact_url_replaces_plex_token_value() {
    let transport = UreqPlexTransport {
        base_url: "http://plex.local:32400".to_string(),
        token: "super-secret-token".to_string(),
    };
    let url = transport
        .url("/search", &[("query", "Song"), ("type", TRACK_TYPE)])
        .expect("url");

    // Sanity: the live URL really does carry the token.
    assert!(url.as_str().contains("super-secret-token"));

    let redacted = redact_url(&url);
    assert!(
        !redacted.contains("super-secret-token"),
        "redacted URL leaked the token: {redacted}"
    );
    assert!(redacted.contains("X-Plex-Token=REDACTED"));
    // Non-sensitive params survive for debugging.
    assert!(redacted.contains("query=Song"));
}

#[test]
fn failing_plex_call_error_does_not_leak_token() {
    // A real transport pointed at an unroutable address fails to connect; the
    // resulting anyhow error chain must not contain the token.
    let mut transport = UreqPlexTransport {
        base_url: "http://127.0.0.1:1".to_string(),
        token: "super-secret-token".to_string(),
    };
    let err = transport
        .get("/identity", &[])
        .expect_err("connection should fail");
    let rendered = format!("{err:#}");
    assert!(
        !rendered.contains("super-secret-token"),
        "error chain leaked the token: {rendered}"
    );
}

/// Build a Plex input DTO the way `service::plex_track_inputs` would for a
/// downloaded audio file: resolved title plus uploader.
fn track(title: &str, uploader: &str) -> PlexTrackInput {
    PlexTrackInput {
        title: title.to_string(),
        uploader: Some(uploader.to_string()),
    }
}
