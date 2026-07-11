use super::*;

fn test_config() -> Config {
    Config {
        target_path: None,
        video_target_path: None,
        allow_local_targets: false,
        staging_dir: None,
        audio_format: "mp3".into(),
        ssh_opts: vec![],
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
        ytdlp_timeout_secs: 1800,
        transfer_timeout_secs: 600,
    }
}

#[test]
fn exe_name_matches_platform() {
    let name = exe_name("yt-dlp");
    if cfg!(target_os = "windows") {
        assert_eq!(name, "yt-dlp.exe");
    } else {
        assert_eq!(name, "yt-dlp");
    }
}

#[test]
fn cache_bin_dir_ends_in_bin() {
    assert!(cache_bin_dir().ends_with("bin"));
}

#[test]
fn resolve_override_errors_on_missing_path() {
    let r = resolve_override_or_path(Some("/no/such/binary/xyz"), "FAKE_PATH", "yt-dlp");
    assert!(r.is_err());
}

#[test]
fn verify_sha256_accepts_matching_digest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    verify_sha256(
        &path,
        "25cb6d61356e5cada4238d160f3a77522e550e27a69758da40cd281c7ef2c8dc",
        "tool",
    )
    .unwrap();
}

#[test]
fn verify_sha256_rejects_mismatched_digest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    let err = verify_sha256(
        &path,
        "0000000000000000000000000000000000000000000000000000000000000000",
        "tool",
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("tool checksum mismatch"));
}

#[test]
fn verify_pin_is_noop_when_unset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    // No pin configured -> always Ok, file untouched.
    verify_pin(&path, None, "tool").unwrap();
    assert!(path.is_file());
}

#[test]
fn verify_pin_accepts_matching_digest_and_keeps_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    verify_pin(
        &path,
        Some("25cb6d61356e5cada4238d160f3a77522e550e27a69758da40cd281c7ef2c8dc"),
        "tool",
    )
    .unwrap();
    assert!(path.is_file(), "matching pin must leave the file in place");
}

#[test]
fn verify_pin_rejects_mismatch_but_keeps_file() {
    // The non-destructive variant (override/PATH binaries) must not delete the
    // user's file.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    let err = verify_pin(
        &path,
        Some("0000000000000000000000000000000000000000000000000000000000000000"),
        "tool",
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("tool checksum mismatch"));
    assert!(
        path.exists(),
        "verify_pin must not delete user-supplied files"
    );
}

#[test]
fn verify_pin_cached_rejects_mismatch_and_removes_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    let err = verify_pin_cached(
        &path,
        Some("0000000000000000000000000000000000000000000000000000000000000000"),
        "tool",
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("tool checksum mismatch"));
    assert!(
        !path.exists(),
        "a cached pin mismatch must delete the offending file so it isn't trusted next run",
    );
}

#[test]
fn verify_pin_cached_is_noop_when_unset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, b"known bytes").unwrap();

    verify_pin_cached(&path, None, "tool").unwrap();
    assert!(path.is_file());
}

#[test]
fn resolve_no_download_returns_found_for_valid_override() {
    let dir = tempfile::tempdir().unwrap();
    let tool = dir.path().join("yt-dlp");
    std::fs::write(&tool, b"binary").unwrap();
    let override_path = tool.display().to_string();

    let resolved = resolve_no_download(Some(&override_path), "YTDLP_PATH", "yt-dlp").unwrap();

    match resolved {
        ResolvedTool::Found(p) => assert_eq!(p, tool),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn resolve_no_download_errors_on_missing_override() {
    // An explicit override pointing at a nonexistent file is a hard error,
    // mirroring the real resolver — the user asked for a specific binary that
    // isn't there.
    let r = resolve_no_download(Some("/no/such/binary/xyz"), "YTDLP_PATH", "yt-dlp");
    assert!(r.is_err());
}

#[test]
fn resolve_no_download_would_bootstrap_when_nothing_present() {
    // No override, and a bin name that is neither on PATH nor in the cache dir,
    // so the only possible outcome is WouldBootstrap (no network, no download).
    let resolved = resolve_no_download(
        None,
        "YTDLP_PATH",
        "ytdl-rmcp-definitely-not-a-real-binary-xyz",
    )
    .unwrap();

    assert!(
        matches!(resolved, ResolvedTool::WouldBootstrap),
        "expected WouldBootstrap, got {resolved:?}"
    );
}

#[test]
fn ensure_ytdlp_enforces_sha256_pin_for_override() {
    let dir = tempfile::tempdir().unwrap();
    let ytdlp = dir.path().join(exe_name("yt-dlp"));
    std::fs::write(&ytdlp, b"not the pinned bytes").unwrap();

    let mut cfg = test_config();
    cfg.ytdlp_path = Some(ytdlp.display().to_string());
    cfg.ytdlp_sha256 =
        Some("0000000000000000000000000000000000000000000000000000000000000000".into());

    let err = ensure_ytdlp(&cfg).unwrap_err().to_string();

    assert!(err.contains("yt-dlp checksum mismatch"));
}

#[test]
fn ensure_enforces_sha256_pins_for_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let ytdlp = dir.path().join(exe_name("yt-dlp"));
    let ffmpeg = dir.path().join(exe_name("ffmpeg"));
    std::fs::write(&ytdlp, b"known bytes").unwrap();
    std::fs::write(&ffmpeg, b"wrong ffmpeg bytes").unwrap();

    let mut cfg = test_config();
    cfg.ytdlp_path = Some(ytdlp.display().to_string());
    cfg.ffmpeg_path = Some(ffmpeg.display().to_string());
    cfg.ytdlp_sha256 =
        Some("25cb6d61356e5cada4238d160f3a77522e550e27a69758da40cd281c7ef2c8dc".into());
    cfg.ffmpeg_sha256 =
        Some("0000000000000000000000000000000000000000000000000000000000000000".into());

    let err = ensure(&cfg).unwrap_err().to_string();

    assert!(err.contains("ffmpeg checksum mismatch"));
}
