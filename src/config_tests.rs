use super::*;

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
        auto_update: true,
        max_age_days: 14,
        update_pre: false,
        ytdlp_path: None,
        ffmpeg_path: None,
        extractor_args: None,
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
