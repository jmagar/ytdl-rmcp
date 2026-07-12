use std::path::PathBuf;

use crate::transfer_queue::{
    list_queue, prune_missing, record_failed_transfer, redact_transfer_error, retry_all, retry_one,
    TransferFailureManifestInput,
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

#[test]
fn redact_transfer_error_masks_common_credential_shapes() {
    let redacted = redact_transfer_error(
        "failed https://user:pass@example.test/path Authorization: Bearer abc123 --token=secret password=hunter2",
    );

    assert!(!redacted.contains("user:pass"));
    assert!(!redacted.contains("abc123"));
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("hunter2"));
    assert!(redacted.contains("https://REDACTED@example.test/path"));
    assert!(redacted.contains("Bearer REDACTED"));
    assert!(redacted.contains("--token=REDACTED"));
    assert!(redacted.contains("password=REDACTED"));
}

#[tokio::test]
async fn retry_missing_staged_kind_returns_structured_failure_and_keeps_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    std::fs::create_dir_all(&staging).unwrap();
    let mut cfg = test_config();
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let entry = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging.clone(),
            targets: vec![("audio".into(), "tootie:/music".into())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();
    let manifest_path = entry.manifest_path.clone();

    let result = retry_one(&cfg, &entry.manifest_id, false).await.unwrap();

    assert_eq!(result.retried, 1);
    assert_eq!(result.completed, 0);
    assert_eq!(result.failed, 1);
    assert!(result.errors[0].contains("staged files no longer match"));
    assert!(staging.exists());
    assert!(manifest_path.exists());
    assert_eq!(list_queue(&cfg).unwrap().entries[0].attempts, 1);
}

#[tokio::test]
async fn retry_missing_staging_marks_manifest_pending_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    std::fs::create_dir_all(staging.join("audio")).unwrap();
    let mut cfg = test_config();
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let entry = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging.clone(),
            targets: vec![("audio".into(), "tootie:/music".into())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();
    std::fs::remove_dir_all(&staging).unwrap();

    let result = retry_one(&cfg, &entry.manifest_id, false).await.unwrap();
    let queued = &list_queue(&cfg).unwrap().entries[0];

    assert_eq!(result.failed, 1);
    assert_eq!(queued.status, "pending");
    assert_eq!(queued.attempts, 1);
    assert!(queued
        .last_error
        .as_deref()
        .unwrap()
        .contains("staging directory no longer exists"));
}

#[tokio::test]
async fn retry_rejects_staging_tree_that_differs_from_manifest_files() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    std::fs::create_dir_all(staging.join("audio/Artist")).unwrap();
    std::fs::write(staging.join("audio/Artist/Song.mp3"), b"song").unwrap();
    std::fs::write(staging.join("audio/Artist/Surprise.mp3"), b"surprise").unwrap();
    let mut cfg = test_config();
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let entry = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging,
            targets: vec![("audio".into(), "tootie:/music".into())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();

    let result = retry_one(&cfg, &entry.manifest_id, false).await.unwrap();
    let queued = &list_queue(&cfg).unwrap().entries[0];

    assert_eq!(result.failed, 1);
    assert!(result.errors[0].contains("staged files no longer match"));
    assert_eq!(queued.status, "pending");
    assert_eq!(queued.attempts, 1);
    assert!(queued
        .last_error
        .as_deref()
        .unwrap()
        .contains("extra=[audio/Artist/Surprise.mp3]"));
}

#[tokio::test]
async fn retry_success_transfers_recorded_files_and_removes_manifest_and_staging() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    let target = dir.path().join("target");
    std::fs::create_dir_all(staging.join("audio/Artist")).unwrap();
    std::fs::write(staging.join("audio/Artist/Song.mp3"), b"song").unwrap();
    let mut cfg = test_config();
    cfg.allow_local_targets = true;
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let entry = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging.clone(),
            targets: vec![("audio".into(), target.display().to_string())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();
    let manifest_path = entry.manifest_path.clone();

    let result = retry_one(&cfg, &entry.manifest_id, false).await.unwrap();

    assert_eq!(result.retried, 1);
    assert_eq!(result.completed, 1);
    assert_eq!(result.failed, 0);
    let copied = find_file_named(&target, "Song.mp3").expect("copied song");
    assert_eq!(std::fs::read(copied).unwrap(), b"song");
    assert!(!manifest_path.exists());
    assert!(!staging.exists());
}

#[tokio::test]
async fn retry_all_reports_mixed_success_and_failure() {
    let dir = tempfile::tempdir().unwrap();
    let good_staging = dir.path().join("good-stage");
    let bad_staging = dir.path().join("bad-stage");
    let target = dir.path().join("target");
    std::fs::create_dir_all(good_staging.join("audio/Artist")).unwrap();
    std::fs::create_dir_all(bad_staging.join("audio/Artist")).unwrap();
    std::fs::write(good_staging.join("audio/Artist/Song.mp3"), b"song").unwrap();
    let mut cfg = test_config();
    cfg.allow_local_targets = true;
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let good = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: good_staging,
            targets: vec![("audio".into(), target.display().to_string())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();
    let bad = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: bad_staging.clone(),
            targets: vec![("audio".into(), target.display().to_string())],
            files: vec![PathBuf::from("audio/Artist/Missing.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();

    let result = retry_all(&cfg, false).await.unwrap();

    assert_eq!(result.retried, 2);
    assert_eq!(result.completed, 1, "{result:?}");
    assert_eq!(result.failed, 1, "{result:?}");
    assert!(!good.manifest_path.exists());
    assert!(bad.manifest_path.exists());
    assert!(list_queue(&cfg)
        .unwrap()
        .entries
        .iter()
        .any(|entry| entry.manifest_id == bad.manifest_id));
}

fn find_file_named(root: &std::path::Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries {
        let path = entry.ok()?.path();
        if path.is_dir() {
            if let Some(found) = find_file_named(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|value| value.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}

#[tokio::test]
async fn retry_rejects_unsafe_manifest_file_paths() {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("stage");
    std::fs::create_dir_all(staging.join("audio/Artist")).unwrap();
    std::fs::write(staging.join("audio/Artist/Song.mp3"), b"song").unwrap();
    let mut cfg = test_config();
    cfg.history_path = Some(dir.path().join("downloads.jsonl").display().to_string());

    let entry = record_failed_transfer(
        &cfg,
        TransferFailureManifestInput {
            staging_path: staging,
            targets: vec![("audio".into(), "tootie:/music".into())],
            files: vec![PathBuf::from("audio/Artist/Song.mp3")],
            last_error: "failed".into(),
        },
    )
    .unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&entry.manifest_path).unwrap()).unwrap();
    manifest["files"] = serde_json::json!(["../Song.mp3"]);
    std::fs::write(
        &entry.manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let result = retry_one(&cfg, &entry.manifest_id, false).await.unwrap();

    assert_eq!(result.failed, 1);
    assert!(result.errors[0].contains("unsafe staged file path"));
}
