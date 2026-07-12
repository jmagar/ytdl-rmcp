use std::path::PathBuf;

use crate::transfer_queue::{
    list_queue, prune_missing, record_failed_transfer, TransferFailureManifestInput,
};

fn test_config() -> crate::config::Config {
    crate::config::Config {
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
        ytdlp_timeout_secs: 5,
        transfer_timeout_secs: 5,
    }
}

#[test]
fn record_failed_transfer_writes_manifest_with_opaque_id() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    std::fs::create_dir_all(staging.join("audio")).unwrap();
    let mut cfg = test_config();
    cfg.staging_dir = Some(dir.path().join("staging-root").display().to_string());
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let entry = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging.clone(),
            targets: vec![("audio".to_string(), "tootie:/music".to_string())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "rsync failed token=secret".to_string(),
        },
    )
    .unwrap();

    assert!(entry.manifest_id.starts_with("tq_"));
    assert_eq!(entry.status, "pending");
    assert!(!entry.last_error.unwrap().contains("secret"));
    assert!(entry.manifest_path.is_file());
}

#[test]
fn prune_missing_removes_only_missing_staging_entries() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    std::fs::create_dir_all(&staging).unwrap();
    let mut cfg = test_config();
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let kept = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging.clone(),
            targets: vec![("audio".into(), "tootie:/music".into())],
            files: vec![PathBuf::from("audio/A/B.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();
    std::fs::remove_dir_all(&staging).unwrap();

    let result = prune_missing(&cfg).unwrap();

    assert_eq!(result.pruned, 1);
    assert!(!kept.manifest_path.exists());
    assert!(list_queue(&cfg).unwrap().entries.is_empty());
}
