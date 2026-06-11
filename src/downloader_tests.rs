use std::time::Duration;

use tokio::process::Command;

use crate::model::SearchResultItem;

use super::*;

#[test]
fn stderr_tail_keeps_last_complete_lines_with_truncation_marker() {
    let input = b"one\ntwo\nthree\nfour\n";
    let tail = stderr_tail_text(input, 10);

    assert!(tail.starts_with("[stderr truncated]\n"));
    assert!(tail.contains("three\nfour"));
    assert!(!tail.contains("one\n"));
}

#[tokio::test]
async fn run_command_reports_timeout() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "sleep 2"]);

    let err = run_command(&mut cmd, Some(Duration::from_millis(50)))
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("timed out after"));
}

#[test]
fn parse_search_json_extracts_youtube_entries() {
    let json = br#"{
      "id": "slow pulp live",
      "title": "slow pulp live",
      "entries": [
        {
          "id": "abc123",
          "title": "Slow Pulp - Falling Apart Live",
          "webpage_url": "https://www.youtube.com/watch?v=abc123",
          "uploader": "Slow Pulp",
          "duration": 215.0,
          "thumbnail": "https://i.ytimg.com/vi/abc123/hqdefault.jpg",
          "view_count": 42000
        },
        null,
        {
          "id": "def456",
          "title": "Slow Pulp - Idaho Live",
          "url": "https://www.youtube.com/watch?v=def456",
          "channel": "Live Room",
          "duration": 188
        }
      ]
    }"#;

    let results = super::parse_search_json(json).unwrap();

    assert_eq!(
        results,
        vec![
            SearchResultItem {
                title: "Slow Pulp - Falling Apart Live".into(),
                url: "https://www.youtube.com/watch?v=abc123".into(),
                video_id: Some("abc123".into()),
                uploader: Some("Slow Pulp".into()),
                duration: Some(215.0),
                thumbnail: Some("https://i.ytimg.com/vi/abc123/hqdefault.jpg".into()),
                view_count: Some(42000),
            },
            SearchResultItem {
                title: "Slow Pulp - Idaho Live".into(),
                url: "https://www.youtube.com/watch?v=def456".into(),
                video_id: Some("def456".into()),
                uploader: Some("Live Room".into()),
                duration: Some(188.0),
                thumbnail: None,
                view_count: None,
            },
        ]
    );
}

#[test]
fn parse_search_json_handles_edge_cases() {
    assert!(super::parse_search_json(b"not json").is_err());
    let missing_entries = super::parse_search_json(br#"{"title":"empty"}"#)
        .unwrap_err()
        .to_string();
    assert!(missing_entries.contains("did not contain an entries array"));
    assert!(missing_entries.contains("title"));

    let json = br#"{
      "entries": [
        { "title": "Missing URL" },
        { "webpage_url": "https://www.youtube.com/watch?v=no-title" },
        {
          "title": "Canonical URL",
          "webpage_url": "https://www.youtube.com/watch?v=canonical",
          "url": "https://youtube.com/shorts/raw"
        },
        {
          "id": "idonly123",
          "title": "ID Only",
          "url": "idonly123"
        }
      ]
    }"#;

    let results = super::parse_search_json(json).unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].title, "Canonical URL");
    assert_eq!(results[0].url, "https://www.youtube.com/watch?v=canonical");
    assert_eq!(results[1].title, "ID Only");
    assert_eq!(results[1].url, "https://www.youtube.com/watch?v=idonly123");
}

#[test]
fn search_query_spec_uses_ytsearch_limit_prefix() {
    assert_eq!(super::search_spec("tiny desk", 7), "ytsearch7:tiny desk");
    assert_eq!(
        super::search_spec("  tiny desk  ", 2),
        "ytsearch2:tiny desk"
    );
}
