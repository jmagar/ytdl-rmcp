use std::path::PathBuf;

use serde_json::json;

use super::*;
use crate::downloader::{ItemResult, MediaFile};
use crate::model::{
    AudioFormat, DownloadMode, SearchPayload, SearchResultItem, Urls, VideoContainer,
};

fn media_file(kind: &'static str, name: &str) -> MediaFile {
    MediaFile {
        path: PathBuf::from(name),
        kind,
        size: 2048,
    }
}

fn test_config() -> Config {
    Config {
        remote: None,
        dest_path: None,
        video_dest_path: None,
        staging_dir: None,
        audio_format: "mp3".into(),
        ssh_opts: vec![],
        archive_dir: None,
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

fn download_input(urls: Urls) -> DownloadInput {
    DownloadInput {
        urls,
        mode: DownloadMode::Audio,
        audio_format: None,
        audio_quality: "0".into(),
        max_height: None,
        container: VideoContainer::Mp4,
        remote: None,
        dest_path: None,
        video_dest_path: None,
        keep_local: false,
        use_archive: false,
        response_format: ResponseFormat::Markdown,
    }
}

#[test]
fn download_payload_marks_files_with_error_as_partial() {
    let results = vec![ItemResult {
        url: "https://example.test/watch".into(),
        title: Some("Half Good".into()),
        files: vec![media_file("video", "Half Good [abc].mp4")],
        error: Some("audio pass failed".into()),
        ..Default::default()
    }];

    let payload = download_payload(&results, "media", &[("video", "/video")], true, None, None);

    let item = &payload["items"][0];
    assert_eq!(item["status"], "partial");
    assert_eq!(item["files"].as_array().unwrap().len(), 1);
    assert_eq!(payload["partial_items"], 1);
    assert_eq!(payload["failed_items"], 0);
}

#[test]
fn markdown_reports_partial_item_without_hiding_files() {
    let payload = json!({
        "transferred": true,
        "transfer_error": null,
        "destination": "media:/video",
        "total_files": 1,
        "total_size": "2.0 KiB",
        "items": [{
            "url": "https://example.test/watch",
            "title": "Half Good",
            "is_playlist": false,
            "status": "partial",
            "error": "audio pass failed",
            "files": [{
                "name": "Half Good [abc].mp4",
                "kind": "video",
                "bytes": 2048
            }]
        }]
    });

    let rendered = render_download_markdown(&payload);

    assert!(rendered.contains("- Half Good - partially completed: audio pass failed"));
    assert!(rendered.contains("[video] Half Good [abc].mp4 (2.0 KiB)"));
    assert!(!rendered.contains("https://example.test/watch - failed"));
}

#[test]
fn render_search_markdown_lists_results_with_urls() {
    let payload = SearchPayload {
        query: "slow pulp".into(),
        limit: 2,
        results: vec![SearchResultItem {
            title: "Slow Pulp - Falling Apart Live".into(),
            url: "https://www.youtube.com/watch?v=abc123".into(),
            video_id: Some("abc123".into()),
            uploader: Some("Slow Pulp".into()),
            duration: Some(215.0),
            thumbnail: None,
            view_count: Some(42000),
        }],
    };

    let rendered = super::render_search_for_test(&payload, ResponseFormat::Markdown);

    assert!(rendered.contains("YouTube search: slow pulp"));
    assert!(rendered.contains("Slow Pulp - Falling Apart Live"));
    assert!(rendered.contains("https://www.youtube.com/watch?v=abc123"));
    assert!(rendered.contains("3:35"));
}

#[test]
fn render_search_json_has_results_array() {
    let payload = SearchPayload {
        query: "slow pulp".into(),
        limit: 1,
        results: Vec::new(),
    };

    let rendered = super::render_search_for_test(&payload, ResponseFormat::Json);
    let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();

    assert_eq!(value["query"], "slow pulp");
    assert_eq!(value["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn invalid_transfer_target_is_rejected_before_tool_resolution() {
    let mut cfg = test_config();
    cfg.ytdlp_path = Some("/definitely/not/a/yt-dlp".into());
    cfg.ffmpeg_path = Some("/definitely/not/a/ffmpeg".into());

    let input = DownloadInput {
        remote: Some("-bad".into()),
        dest_path: Some("/music".into()),
        response_format: ResponseFormat::Json,
        ..download_input(Urls::One("https://example.test/watch".into()))
    };

    let err = run_download(&cfg, input).await.unwrap_err().to_string();

    assert!(err.contains("SSH remote must not start with '-'"));
    assert!(!err.contains("YTDLP_PATH"));
    assert!(!err.contains("FFMPEG_PATH"));
}

#[tokio::test]
async fn run_download_json_reports_partial_status_with_fake_runtime() {
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    static PATH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    let staging = dir.path().join("staging");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&staging).unwrap();
    let fake = write_fake_runtime(&bin);

    let old_path = std::env::var_os("PATH");
    let mut path_entries = vec![bin.clone()];
    if let Some(old_path) = &old_path {
        path_entries.extend(std::env::split_paths(old_path));
    }
    std::env::set_var("PATH", std::env::join_paths(path_entries).unwrap());

    let mut cfg = test_config();
    cfg.ytdlp_path = Some(fake.ytdlp.display().to_string());
    cfg.ffmpeg_path = Some(fake.ffmpeg.display().to_string());
    cfg.staging_dir = Some(staging.display().to_string());

    let input = DownloadInput {
        mode: DownloadMode::Both,
        audio_format: Some(AudioFormat::Mp3),
        remote: Some("media".into()),
        dest_path: Some("/audio".into()),
        video_dest_path: Some("/video".into()),
        response_format: ResponseFormat::Json,
        ..download_input(Urls::One("https://example.test/watch".into()))
    };

    let output = run_download(&cfg, input).await;

    if let Some(old_path) = old_path {
        std::env::set_var("PATH", old_path);
    } else {
        std::env::remove_var("PATH");
    }

    let value: serde_json::Value = serde_json::from_str(&output.unwrap()).unwrap();
    assert_eq!(value["transferred"], true);
    assert_eq!(value["partial_items"], 1);
    assert_eq!(value["failed_items"], 0);
    assert_eq!(value["total_files"], 1);
    assert_eq!(value["destinations"][0]["kind"], "video");
    assert_eq!(value["items"][0]["status"], "partial");
    assert_eq!(value["items"][0]["error"], "audio pass failed");
    assert_eq!(value["items"][0]["files"][0]["kind"], "video");
}

struct FakeRuntime {
    ytdlp: PathBuf,
    ffmpeg: PathBuf,
}

#[cfg(unix)]
fn write_fake_runtime(bin: &std::path::Path) -> FakeRuntime {
    use std::os::unix::fs::PermissionsExt;

    let ytdlp = bin.join("yt-dlp");
    std::fs::write(
        &ytdlp,
        r#"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    shift
    out="$1"
  fi
  shift || true
done
case "$out" in
  */video/*)
    staging="${out%%/video/*}"
    file="$staging/video/Fake Artist/Fake Title [vid123].mp4"
    mkdir -p "$(dirname "$file")"
    printf "video bytes" > "$file"
    printf 'vid123\037Fake Title\037Fake Artist\03712.5\037%s\n' "$file"
    ;;
  */audio/*)
    printf 'audio pass failed\n' >&2
    exit 33
    ;;
  *)
    printf 'unexpected output template: %s\n' "$out" >&2
    exit 34
    ;;
esac
"#,
    )
    .unwrap();
    let ffmpeg = bin.join("ffmpeg");
    std::fs::write(&ffmpeg, b"#!/bin/sh\nexit 0\n").unwrap();
    let ssh = bin.join("ssh");
    std::fs::write(&ssh, b"#!/bin/sh\nexit 0\n").unwrap();
    let rsync = bin.join("rsync");
    std::fs::write(&rsync, b"#!/bin/sh\nexit 0\n").unwrap();
    for path in [&ytdlp, &ffmpeg, &ssh, &rsync] {
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
    FakeRuntime { ytdlp, ffmpeg }
}

#[cfg(windows)]
fn write_fake_runtime(bin: &std::path::Path) -> FakeRuntime {
    let ytdlp = bin.join("yt-dlp.cmd");
    let ytdlp_ps1 = bin.join("fake-ytdlp.ps1");
    std::fs::write(
        &ytdlp,
        "@powershell -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-ytdlp.ps1\" %*\r\n",
    )
    .unwrap();
    std::fs::write(
        &ytdlp_ps1,
        r#"$out = ""
for ($i = 0; $i -lt $args.Count; $i++) {
  if ($args[$i] -eq "-o" -and ($i + 1) -lt $args.Count) {
    $out = $args[$i + 1]
    $i++
  }
}
if ($out -like "*\video\*") {
  $staging = $out -replace "\\video\\.*$", ""
  $file = Join-Path $staging "video\Fake Artist\Fake Title [vid123].mp4"
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $file) | Out-Null
  Set-Content -NoNewline -Path $file -Value "video bytes"
  Write-Output ("vid123{0}Fake Title{0}Fake Artist{0}12.5{0}{1}" -f [char]31, $file)
} elseif ($out -like "*\audio\*") {
  [Console]::Error.WriteLine("audio pass failed")
  exit 33
} else {
  [Console]::Error.WriteLine("unexpected output template: $out")
  exit 34
}
"#,
    )
    .unwrap();
    let ffmpeg = bin.join("ffmpeg.cmd");
    std::fs::write(&ffmpeg, "@exit /b 0\r\n").unwrap();
    std::fs::write(bin.join("ssh.cmd"), "@exit /b 0\r\n").unwrap();
    std::fs::write(bin.join("rsync.cmd"), "@exit /b 0\r\n").unwrap();
    FakeRuntime { ytdlp, ffmpeg }
}
