use super::*;
use crate::config::Config;
use crate::model::{IdentifyInput, Paths, ResponseFormat, StatsInput};

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
        ytdlp_timeout_secs: 5,
        transfer_timeout_secs: 5,
    }
}

#[test]
fn run_stats_json_summarizes_history_and_recent_entries() {
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("downloads.jsonl");
    std::fs::write(
        &history,
        concat!(
            "{\"timestamp\":\"2026-06-11T01:00:00Z\",\"mode\":\"audio\",\"remote\":\"tootie\",\"transferred\":true,\"total_files\":1,\"total_bytes\":10,\"items\":[{\"status\":\"ok\",\"title\":\"Song A\",\"uploader\":\"Artist A\",\"files\":[{\"kind\":\"audio\",\"bytes\":10}]}]}\n",
            "{\"timestamp\":\"2026-06-11T02:00:00Z\",\"mode\":\"video\",\"remote\":\"tootie\",\"transferred\":true,\"total_files\":2,\"total_bytes\":50,\"items\":[{\"status\":\"ok\",\"title\":\"Video B\",\"uploader\":\"Artist B\",\"files\":[{\"kind\":\"video\",\"bytes\":30},{\"kind\":\"audio\",\"bytes\":20}]}]}\n"
        ),
    )
    .unwrap();

    let mut cfg = test_config();
    cfg.history_path = Some(history.display().to_string());

    let output = run_stats(
        &cfg,
        StatsInput {
            limit: 1,
            response_format: ResponseFormat::Json,
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["total_downloads"], 2);
    assert_eq!(value["total_files"], 3);
    assert_eq!(value["total_bytes"], 60);
    assert_eq!(value["by_kind"]["audio"]["files"], 2);
    assert_eq!(value["by_kind"]["audio"]["downloads"], 2);
    assert_eq!(value["by_kind"]["audio"]["calls"], 2);
    assert_eq!(value["by_kind"]["video"]["files"], 1);
    assert_eq!(value["by_kind"]["video"]["downloads"], 1);
    assert_eq!(value["by_kind"]["video"]["calls"], 1);
    assert_eq!(value["by_uploader"]["Artist B"]["downloads"], 1);
    assert_eq!(value["by_uploader"]["Artist B"]["calls"], 1);
    assert_eq!(value["by_uploader"]["Artist B"]["items"], 1);
    assert_eq!(value["recent"].as_array().unwrap().len(), 1);
    assert_eq!(value["recent"][0]["items"][0]["title"], "Video B");
}

#[test]
fn run_stats_json_skips_malformed_lines_and_counts_uploader_calls_once() {
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("downloads.jsonl");
    std::fs::write(
        &history,
        concat!(
            "{\"timestamp\":\"2026-06-11T01:00:00Z\",\"mode\":\"audio\",\"remote\":\"tootie\",\"transferred\":true,\"total_files\":1,\"total_bytes\":10,\"items\":[{\"status\":\"ok\",\"title\":\"Song A\",\"uploader\":\"Artist A\",\"files\":[{\"kind\":\"audio\",\"bytes\":10}]}]}\n",
            "this is not json\n",
            "{\"timestamp\":\"2026-06-11T02:00:00Z\",\"mode\":\"both\",\"remote\":\"tootie\",\"transferred\":true,\"total_files\":3,\"total_bytes\":55,\"items\":[{\"status\":\"ok\",\"title\":\"Video B\",\"uploader\":\"Artist B\",\"files\":[{\"kind\":\"video\",\"bytes\":30},{\"kind\":\"audio\",\"bytes\":20}]},{\"status\":\"ok\",\"title\":\"Clip B\",\"uploader\":\"Artist B\",\"files\":[{\"kind\":\"audio\",\"bytes\":5}]}]}\n"
        ),
    )
    .unwrap();

    let mut cfg = test_config();
    cfg.history_path = Some(history.display().to_string());

    let output = run_stats(
        &cfg,
        StatsInput {
            limit: 10,
            response_format: ResponseFormat::Json,
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["total_downloads"], 2);
    assert_eq!(value["skipped_entries"], 1);
    assert_eq!(value["total_files"], 4);
    assert_eq!(value["total_bytes"], 65);
    assert_eq!(value["by_uploader"]["Artist B"]["downloads"], 1);
    assert_eq!(value["by_uploader"]["Artist B"]["calls"], 1);
    assert_eq!(value["by_uploader"]["Artist B"]["items"], 2);
    assert_eq!(value["by_uploader"]["Artist B"]["files"], 3);
    assert_eq!(value["recent"].as_array().unwrap().len(), 2);
    assert_eq!(value["recent"][0]["items"][0]["title"], "Video B");
}

#[tokio::test]
async fn run_identify_requires_acoustid_client_key() {
    let cfg = test_config();

    let err = run_identify(
        &std::sync::Arc::new(cfg),
        IdentifyInput {
            paths: Paths::One("/tmp/song.mp3".into()),
            write_tags: false,
            response_format: ResponseFormat::Json,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(err.contains("YTDLP_ACOUSTID_CLIENT_KEY"));
}
