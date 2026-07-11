use super::*;
use std::sync::{Mutex, MutexGuard, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

/// A Config with everything empty/default, for exercising pure methods without
/// touching the process environment.
fn blank() -> Config {
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
        auto_update: true,
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
fn all_ssh_opts_prepends_forced_defaults() {
    let cfg = blank();
    let opts = cfg.all_ssh_opts();
    // BatchMode + StrictHostKeyChecking are always forced first.
    assert_eq!(
        &opts[..4],
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=accept-new"
        ]
    );
}

#[test]
fn all_ssh_opts_appends_user_extras() {
    let mut cfg = blank();
    cfg.ssh_opts = vec!["-p".into(), "2222".into()];
    let opts = cfg.all_ssh_opts();
    assert_eq!(opts.len(), DEFAULT_SSH_OPTS.len() + 2);
    assert_eq!(&opts[opts.len() - 2..], &["-p", "2222"]);
}

#[test]
fn default_timeouts_are_sane_for_long_downloads() {
    let cfg = blank();
    assert_eq!(cfg.ytdlp_timeout().as_secs(), 1800);
    assert_eq!(cfg.transfer_timeout().as_secs(), 600);
}

#[test]
fn checksum_pin_normalizes_blank_and_case() {
    assert_eq!(normalize_sha256_pin(""), None);
    assert_eq!(normalize_sha256_pin(" not-hex "), None);
    assert_eq!(
        normalize_sha256_pin(" ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789 "),
        Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into())
    );
}

#[test]
fn parse_ssh_opts_preserves_quoted_values() {
    // Safe options keep their (possibly space-containing) quoted values intact.
    assert_eq!(
        parse_ssh_opts("-i '/home/me/media key' -o 'ConnectTimeout=10'"),
        vec!["-i", "/home/me/media key", "-o", "ConnectTimeout=10"]
    );
}

#[test]
fn parse_ssh_opts_strips_command_execution_options() {
    // Split form: `-o KEY=VALUE`. All three dangerous keys are dropped (key and
    // value tokens both consumed), case-insensitively, while a safe `-o` option
    // and a non-`-o` flag survive.
    assert_eq!(
        parse_ssh_opts(
            "-o ProxyCommand=evil -o ConnectTimeout=10 \
             -o localcommand=touch\\ pwned -o PermitLocalCommand=yes -p 2222"
        ),
        vec!["-o", "ConnectTimeout=10", "-p", "2222"]
    );
}

#[test]
fn parse_ssh_opts_strips_glued_command_execution_options() {
    // Glued form: `-oKEY=VALUE`.
    assert_eq!(
        parse_ssh_opts(
            "-oProxyCommand=evil -oConnectTimeout=10 \
             -oLocalCommand=evil -oPermitLocalCommand=yes"
        ),
        vec!["-oConnectTimeout=10"]
    );
}

#[test]
fn parse_ssh_opts_preserves_safe_options_and_defaults() {
    // A typical safe override set passes through untouched, and combines cleanly
    // with the forced DEFAULT_SSH_OPTS via all_ssh_opts().
    let parsed = parse_ssh_opts("-i /home/me/key -o ConnectTimeout=10 -p 2222");
    assert_eq!(
        parsed,
        vec![
            "-i",
            "/home/me/key",
            "-o",
            "ConnectTimeout=10",
            "-p",
            "2222"
        ]
    );

    let mut cfg = blank();
    cfg.ssh_opts = parsed;
    let opts = cfg.all_ssh_opts();
    assert_eq!(
        &opts[..4],
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=accept-new"
        ]
    );
    assert_eq!(
        &opts[4..],
        &[
            "-i",
            "/home/me/key",
            "-o",
            "ConnectTimeout=10",
            "-p",
            "2222"
        ]
    );
}

#[test]
fn from_env_result_rejects_invalid_sha256_pins() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var("YTDLP_SHA256", "not-a-sha");

    let err = Config::from_env_result().unwrap_err().to_string();

    assert!(err.contains("YTDLP_SHA256"));
    assert!(err.contains("64 lowercase or uppercase hex characters"));
    clear_test_env();
}

#[test]
fn from_env_result_rejects_malformed_ssh_opts() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var("YTDLP_SSH_OPTS", "-i '/tmp/missing end");

    let err = Config::from_env_result().unwrap_err().to_string();

    assert!(err.contains("YTDLP_SSH_OPTS"));
    clear_test_env();
}

#[test]
fn from_env_result_rejects_invalid_and_zero_timeouts() {
    let _guard = env_lock();
    clear_test_env();

    std::env::set_var("YTDLP_TIMEOUT_SECS", "eventually");
    let err = Config::from_env_result().unwrap_err().to_string();
    assert!(err.contains("YTDLP_TIMEOUT_SECS"));
    assert!(err.contains("positive integer"));

    std::env::set_var("YTDLP_TIMEOUT_SECS", "1");
    std::env::set_var("YTDLP_TRANSFER_TIMEOUT_SECS", "0");
    let err = Config::from_env_result().unwrap_err().to_string();
    assert!(err.contains("YTDLP_TRANSFER_TIMEOUT_SECS"));
    assert!(err.contains("greater than zero"));

    clear_test_env();
}

#[test]
fn from_env_result_wires_runtime_env_values() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var("YTDLP_TARGET_PATH", "media:/audio");
    std::env::set_var("YTDLP_VIDEO_TARGET_PATH", "media:/video");
    std::env::set_var("YTDLP_ALLOW_LOCAL_TARGETS", "true");
    std::env::set_var("YTDLP_AUDIO_FORMAT", "opus");
    std::env::set_var("YTDLP_SSH_OPTS", "-i '/home/me/media key' -p 2222");
    std::env::set_var("YTDLP_HISTORY_PATH", "/tmp/ytdl-history.jsonl");
    std::env::set_var("YTDLP_PLEX_URL", "http://plex.local:32400");
    std::env::set_var("YTDLP_PLEX_TOKEN", "plex-token");
    std::env::set_var("YTDLP_PLEX_PLAYLIST", "Downloads");
    std::env::set_var("YTDLP_ACOUSTID_CLIENT_KEY", "acoustid-key");
    std::env::set_var("FPCALC_PATH", "/opt/bin/fpcalc");
    std::env::set_var("YTDLP_MUSICBRAINZ_CONTACT", "https://example.test/contact");
    std::env::set_var(
        "YTDLP_SHA256",
        "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789",
    );
    std::env::set_var("YTDLP_TIMEOUT_SECS", "77");
    std::env::set_var("YTDLP_TRANSFER_TIMEOUT_SECS", "88");

    let cfg = Config::from_env_result().unwrap();

    assert_eq!(cfg.target_path.as_deref(), Some("media:/audio"));
    assert_eq!(cfg.video_target_path.as_deref(), Some("media:/video"));
    assert!(cfg.allow_local_targets);
    assert_eq!(cfg.audio_format, "opus");
    assert_eq!(cfg.ssh_opts, vec!["-i", "/home/me/media key", "-p", "2222"]);
    assert_eq!(cfg.history_path.as_deref(), Some("/tmp/ytdl-history.jsonl"));
    assert_eq!(cfg.plex_url.as_deref(), Some("http://plex.local:32400"));
    assert_eq!(cfg.plex_token.as_deref(), Some("plex-token"));
    assert_eq!(cfg.plex_playlist.as_deref(), Some("Downloads"));
    assert!(cfg.clean_metadata);
    assert_eq!(cfg.acoustid_client_key.as_deref(), Some("acoustid-key"));
    assert_eq!(cfg.fpcalc_path.as_deref(), Some("/opt/bin/fpcalc"));
    assert_eq!(
        cfg.musicbrainz_contact.as_deref(),
        Some("https://example.test/contact")
    );
    assert_eq!(
        cfg.ytdlp_sha256.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
    );
    assert_eq!(cfg.ytdlp_timeout_secs, 77);
    assert_eq!(cfg.transfer_timeout_secs, 88);

    clear_test_env();
}

#[test]
fn from_env_result_composes_legacy_ssh_targets_explicitly() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var("YTDLP_REMOTE", "nas");
    std::env::set_var("YTDLP_REMOTE_PATH", "/music");
    std::env::set_var("YTDLP_VIDEO_REMOTE_PATH", "/videos");

    let cfg = Config::from_env_result().unwrap();

    assert_eq!(cfg.target_path.as_deref(), Some("ssh:nas:/music"));
    assert_eq!(cfg.video_target_path.as_deref(), Some("ssh:nas:/videos"));
    clear_test_env();
}

#[test]
fn from_env_strips_dangerous_ssh_opts_end_to_end() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var(
        "YTDLP_SSH_OPTS",
        "-o ProxyCommand=evil -oPermitLocalCommand=yes -o ConnectTimeout=10 -p 2222",
    );

    // from_env() is the #[cfg(test)]-only panicking wrapper; verify it still
    // works in test builds and that the dangerous options are stripped.
    let cfg = Config::from_env();

    assert_eq!(cfg.ssh_opts, vec!["-o", "ConnectTimeout=10", "-p", "2222"]);
    clear_test_env();
}

#[test]
fn from_env_result_defaults_plex_playlist_when_plex_is_configured() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var("YTDLP_PLEX_URL", "http://plex.local:32400");
    std::env::set_var("YTDLP_PLEX_TOKEN", "plex-token");

    let cfg = Config::from_env_result().unwrap();

    assert_eq!(cfg.plex_playlist.as_deref(), Some(DEFAULT_PLEX_PLAYLIST));
    clear_test_env();
}

#[test]
fn from_env_result_can_disable_metadata_cleanup() {
    let _guard = env_lock();
    clear_test_env();
    std::env::set_var("YTDLP_CLEAN_METADATA", "0");

    let cfg = Config::from_env_result().unwrap();

    assert!(!cfg.clean_metadata);
    clear_test_env();
}

fn clear_test_env() {
    for key in [
        "YTDLP_TARGET_PATH",
        "YTDLP_VIDEO_TARGET_PATH",
        "YTDLP_ALLOW_LOCAL_TARGETS",
        "YTDLP_REMOTE",
        "YTDLP_REMOTE_PATH",
        "YTDLP_VIDEO_REMOTE_PATH",
        "YTDLP_AUDIO_FORMAT",
        "YTDLP_SSH_OPTS",
        "YTDLP_HISTORY_PATH",
        "YTDLP_PLEX_URL",
        "YTDLP_PLEX_TOKEN",
        "YTDLP_PLEX_PLAYLIST",
        "YTDLP_CLEAN_METADATA",
        "YTDLP_ACOUSTID_CLIENT_KEY",
        "FPCALC_PATH",
        "YTDLP_MUSICBRAINZ_CONTACT",
        "YTDLP_SHA256",
        "FFMPEG_SHA256",
        "YTDLP_TIMEOUT_SECS",
        "YTDLP_TRANSFER_TIMEOUT_SECS",
    ] {
        std::env::remove_var(key);
    }
}
