use std::path::PathBuf;

use serde_json::json;

use super::*;
use crate::downloader::{ItemResult, MediaFile};
use crate::model::{ResponseFormat, SearchPayload, SearchResultItem};

fn media_file(kind: &'static str, name: &str) -> MediaFile {
    MediaFile {
        path: PathBuf::from(name),
        kind,
        size: 2048,
        title: Some(name.to_string()),
        video_id: None,
        uploader: None,
        duration: None,
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

    let payload = download_payload(&results, &[("video", "media:/video")], true, None, None);

    let item = &payload["items"][0];
    assert_eq!(item["status"], "partial");
    assert_eq!(item["files"].as_array().unwrap().len(), 1);
    assert_eq!(payload["partial_items"], 1);
    assert_eq!(payload["failed_items"], 0);
}

#[test]
fn download_payload_does_not_classify_explicit_rclone_as_legacy_ssh() {
    let results = vec![ItemResult {
        url: "https://example.test/watch".into(),
        title: Some("Song".into()),
        files: vec![media_file("audio", "Song [abc].mp3")],
        ..Default::default()
    }];

    let payload = download_payload(
        &results,
        &[("audio", "rclone:gdrive:/Music/ytdl")],
        true,
        None,
        None,
    );

    assert_eq!(payload["remote"], serde_json::Value::Null);
    assert_eq!(payload["dest_path"], "rclone:gdrive:/Music/ytdl");
    assert_eq!(payload["target_path"], "rclone:gdrive:/Music/ytdl");
    assert_eq!(
        payload["destinations"][0]["dest_path"],
        "rclone:gdrive:/Music/ytdl"
    );
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

    let rendered = render_search_for_test(&payload, ResponseFormat::Markdown);

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

    let rendered = render_search_for_test(&payload, ResponseFormat::Json);
    let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();

    assert_eq!(value["query"], "slow pulp");
    assert_eq!(value["results"].as_array().unwrap().len(), 0);
}

#[test]
fn plex_playlist_markdown_includes_missing_tracks_and_errors() {
    let rendered = render_plex_playlist_markdown(&json!({
        "playlist": "Road Mix",
        "matched": 1,
        "added": 0,
        "already_present": 1,
        "missing": [
            { "title": "Ghost Track", "uploader": "No Artist" }
        ],
        "errors": ["Plex search failed"]
    }));

    assert!(rendered.contains("Road Mix: 1 matched, 0 added, 1 already present."));
    assert!(rendered.contains("Missing tracks:"));
    assert!(rendered.contains("- Ghost Track (No Artist)"));
    assert!(rendered.contains("Errors:"));
    assert!(rendered.contains("- Plex search failed"));
}

#[test]
fn transfer_queue_retry_markdown_includes_errors() {
    let rendered = render_transfer_queue_markdown(&json!({
        "retried": 1,
        "completed": 0,
        "failed": 1,
        "entries": [],
        "errors": ["no staged target directories were found"]
    }));

    assert!(rendered.contains("Retried 1 transfer queue item(s): 0 completed, 1 failed."));
    assert!(rendered.contains("Errors:"));
    assert!(rendered.contains("- no staged target directories were found"));
}
