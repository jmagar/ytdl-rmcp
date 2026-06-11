use super::*;

fn test_config() -> Config {
    Config {
        remote: None,
        dest_path: None,
        video_dest_path: None,
        staging_dir: None,
        audio_format: "mp3".into(),
        ssh_opts: vec![],
        archive_dir: None,
        history_path: None,
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
