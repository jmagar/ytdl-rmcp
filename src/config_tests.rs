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
        remote: None,
        dest_path: None,
        video_dest_path: None,
        staging_dir: None,
        audio_format: "mp3".into(),
        ssh_opts: vec![],
        archive_dir: None,
        history_path: None,
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
    assert_eq!(
        parse_ssh_opts("-i '/home/me/media key' -o 'ProxyCommand=ssh jump nc %h %p'"),
        vec![
            "-i",
            "/home/me/media key",
            "-o",
            "ProxyCommand=ssh jump nc %h %p"
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
    std::env::set_var("YTDLP_REMOTE", "media");
    std::env::set_var("YTDLP_REMOTE_PATH", "/audio");
    std::env::set_var("YTDLP_VIDEO_REMOTE_PATH", "/video");
    std::env::set_var("YTDLP_AUDIO_FORMAT", "opus");
    std::env::set_var("YTDLP_SSH_OPTS", "-i '/home/me/media key' -p 2222");
    std::env::set_var("YTDLP_HISTORY_PATH", "/tmp/ytdl-history.jsonl");
    std::env::set_var(
        "YTDLP_SHA256",
        "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789",
    );
    std::env::set_var("YTDLP_TIMEOUT_SECS", "77");
    std::env::set_var("YTDLP_TRANSFER_TIMEOUT_SECS", "88");

    let cfg = Config::from_env_result().unwrap();

    assert_eq!(cfg.remote.as_deref(), Some("media"));
    assert_eq!(cfg.dest_path.as_deref(), Some("/audio"));
    assert_eq!(cfg.video_dest_path.as_deref(), Some("/video"));
    assert_eq!(cfg.audio_format, "opus");
    assert_eq!(cfg.ssh_opts, vec!["-i", "/home/me/media key", "-p", "2222"]);
    assert_eq!(cfg.history_path.as_deref(), Some("/tmp/ytdl-history.jsonl"));
    assert_eq!(
        cfg.ytdlp_sha256.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
    );
    assert_eq!(cfg.ytdlp_timeout_secs, 77);
    assert_eq!(cfg.transfer_timeout_secs, 88);

    clear_test_env();
}

fn clear_test_env() {
    for key in [
        "YTDLP_REMOTE",
        "YTDLP_REMOTE_PATH",
        "YTDLP_VIDEO_REMOTE_PATH",
        "YTDLP_AUDIO_FORMAT",
        "YTDLP_SSH_OPTS",
        "YTDLP_HISTORY_PATH",
        "YTDLP_SHA256",
        "FFMPEG_SHA256",
        "YTDLP_TIMEOUT_SECS",
        "YTDLP_TRANSFER_TIMEOUT_SECS",
    ] {
        std::env::remove_var(key);
    }
}
