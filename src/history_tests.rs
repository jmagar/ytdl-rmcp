use std::io::Write;
use std::path::Path;

use serde_json::{json, Value};

use super::*;
use crate::config::Config;
use crate::model::DownloadMode;
use crate::service::{DownloadFile, DownloadItem, DownloadPayload, DownloadStatus};

/// Minimal Config pointing the ledger at `path`. Only `history_path` matters
/// for these tests; everything else is a benign default.
fn config_with_history(path: &Path) -> Config {
    Config {
        target_path: None,
        video_target_path: None,
        allow_local_targets: false,
        staging_dir: None,
        audio_format: "mp3".into(),
        ssh_opts: vec![],
        archive_dir: None,
        history_path: Some(path.to_string_lossy().into_owned()),
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

/// A representative download payload with one item, one uploader, and two files
/// of distinct kinds.
fn sample_payload(uploader: &str, transferred: bool) -> DownloadPayload {
    let file = |kind: &'static str, bytes: u64| DownloadFile {
        name: None,
        kind,
        bytes,
        title: None,
        video_id: None,
        uploader: None,
        duration: None,
    };
    DownloadPayload {
        transferred,
        transfer_error: None,
        remote: Some("host".into()),
        dest_path: "/music".into(),
        target_path: "host:/music".into(),
        destination: None,
        destinations: Vec::new(),
        staging_kept_at: None,
        total_files: 2,
        total_bytes: 3072,
        total_size: "3.0 KiB".into(),
        partial_items: 0,
        failed_items: 0,
        items: vec![DownloadItem {
            url: String::new(),
            status: DownloadStatus::Ok,
            title: Some("Some Track".into()),
            video_id: None,
            duration: None,
            uploader: Some(uploader.to_string()),
            is_playlist: false,
            error: None,
            files: vec![file("audio", 1024), file("thumbnail", 2048)],
        }],
        metadata_retag: None,
        plex_playlist: None,
        plex_playlist_error: None,
        history_error: None,
    }
}

#[test]
fn append_then_stats_round_trips_a_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");
    let cfg = config_with_history(&path);

    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist A", true)).unwrap();

    let stats = stats_payload(&cfg, 10).unwrap();
    assert_eq!(stats["total_downloads"].as_u64(), Some(1));
    assert_eq!(stats["total_files"].as_u64(), Some(2));
    assert_eq!(stats["total_bytes"].as_u64(), Some(3072));
    assert_eq!(stats["skipped_entries"].as_u64(), Some(0));

    let recent = stats["recent"].as_array().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["items"][0]["title"].as_str(), Some("Some Track"));
}

#[test]
fn round_trips_a_non_transferred_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");
    let cfg = config_with_history(&path);

    append_download(
        &cfg,
        DownloadMode::Audio,
        &sample_payload("Artist B", false),
    )
    .unwrap();

    let stats = stats_payload(&cfg, 10).unwrap();
    assert_eq!(stats["total_downloads"].as_u64(), Some(1));
    let recent = stats["recent"].as_array().unwrap();
    // `transferred: false` survives the JSONL round-trip intact.
    assert_eq!(recent[0]["transferred"].as_bool(), Some(false));
}

#[test]
fn malformed_lines_are_skipped_not_panicked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");
    let cfg = config_with_history(&path);

    // One good record, then assorted garbage the reader must tolerate.
    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist C", true)).unwrap();
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "this is not json").unwrap();
        writeln!(f, "{{ \"truncated\": ").unwrap();
        writeln!(f).unwrap(); // blank line: ignored, not counted as skipped
        writeln!(f, "[1, 2, 3").unwrap();
    }

    let stats = stats_payload(&cfg, 10).unwrap();
    assert_eq!(stats["total_downloads"].as_u64(), Some(1));
    // Three malformed lines skipped; the blank line is silently ignored.
    assert_eq!(stats["skipped_entries"].as_u64(), Some(3));
}

#[test]
fn aggregation_counts_by_kind_and_uploader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");
    let cfg = config_with_history(&path);

    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist A", true)).unwrap();
    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist A", true)).unwrap();
    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist B", true)).unwrap();

    let stats = stats_payload(&cfg, 0).unwrap();
    assert_eq!(stats["total_downloads"].as_u64(), Some(3));
    assert_eq!(stats["total_files"].as_u64(), Some(6));
    assert_eq!(stats["total_bytes"].as_u64(), Some(9216));

    // by_kind: each download contributes one audio + one thumbnail file.
    let by_kind = &stats["by_kind"];
    assert_eq!(by_kind["audio"]["files"].as_u64(), Some(3));
    assert_eq!(by_kind["audio"]["bytes"].as_u64(), Some(3072));
    assert_eq!(by_kind["thumbnail"]["files"].as_u64(), Some(3));
    // One call per download that touched the kind.
    assert_eq!(by_kind["audio"]["calls"].as_u64(), Some(3));

    // by_uploader: Artist A appears in two downloads, Artist B in one.
    let by_uploader = &stats["by_uploader"];
    assert_eq!(by_uploader["Artist A"]["calls"].as_u64(), Some(2));
    assert_eq!(by_uploader["Artist B"]["calls"].as_u64(), Some(1));
    assert_eq!(by_uploader["Artist A"]["files"].as_u64(), Some(4));
}

#[test]
fn downloads_alias_mirrors_calls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");
    let cfg = config_with_history(&path);

    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist A", true)).unwrap();

    let stats = stats_payload(&cfg, 0).unwrap();
    let audio = &stats["by_kind"]["audio"];
    // The `downloads` compatibility alias is always equal to `calls`.
    assert_eq!(audio["downloads"], audio["calls"]);
    let artist = &stats["by_uploader"]["Artist A"];
    assert_eq!(artist["downloads"], artist["calls"]);
}

#[test]
fn rotation_bounds_the_ledger_and_keeps_recent_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");

    // Write well past the rotation trigger, tagging each line with an index so
    // we can confirm the *newest* entries are the ones retained.
    let total = ROTATE_TRIGGER_ENTRIES + 50;
    {
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..total {
            let entry = json!({
                "timestamp": "2024-01-01T00:00:00Z",
                "mode": "audio",
                "seq": i,
                "total_files": 0,
                "total_bytes": 0,
                "items": [],
            });
            writeln!(f, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
        }
    }
    assert!(total > ROTATE_TRIGGER_ENTRIES);

    // Trigger rotation through the public append path (the cap is enforced as a
    // best-effort side effect of appending).
    let cfg = config_with_history(&path);
    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist Z", true)).unwrap();

    // The file is now bounded to the cap (plus the line we just appended).
    let contents = std::fs::read_to_string(&path).unwrap();
    let kept: Vec<&str> = contents.lines().collect();
    assert!(
        kept.len() <= MAX_HISTORY_ENTRIES + 1,
        "ledger not bounded: {} lines",
        kept.len()
    );

    // The oldest entries were dropped: seq 0 must be gone, and the appended
    // record (the newest) must be present.
    let first: Value = serde_json::from_str(kept.first().unwrap()).unwrap();
    assert!(
        first["seq"].as_u64().unwrap() > 0,
        "oldest entries were not trimmed"
    );
    assert_eq!(
        kept.iter()
            .filter(|l| l.contains("\"uploader\":\"Artist Z\""))
            .count(),
        1,
        "newest appended record was lost"
    );

    // No temp file is left behind after a successful rotation.
    assert!(!path.with_extension("jsonl.tmp").exists());

    // Stats still parse the trimmed ledger cleanly.
    let stats = stats_payload(&cfg, 5).unwrap();
    assert_eq!(stats["skipped_entries"].as_u64(), Some(0));
}

/// Seed `path` with `count` valid, indexed ledger lines.
fn seed_ledger(path: &Path, count: usize) {
    let mut f = std::fs::File::create(path).unwrap();
    for i in 0..count {
        let entry = json!({
            "timestamp": "2024-01-01T00:00:00Z",
            "mode": "audio",
            "seq": i,
            "total_files": 0,
            "total_bytes": 0,
            "items": [],
        });
        writeln!(f, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
    }
}

/// Count `*.tmp` files left in `dir` — there must be none after rotation.
fn orphan_tmp_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .count()
}

#[test]
fn concurrent_appends_keep_the_ledger_valid_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");

    // Seed just past the trigger so EVERY concurrent append crosses the rotation
    // threshold and races to rewrite the file.
    seed_ledger(&path, ROTATE_TRIGGER_ENTRIES + 1);

    let cfg = config_with_history(&path);
    const N: usize = 8;
    std::thread::scope(|scope| {
        for t in 0..N {
            let cfg = &cfg;
            scope.spawn(move || {
                let uploader = format!("Worker {t}");
                let payload = sample_payload(&uploader, true);
                append_download(cfg, DownloadMode::Audio, &payload).unwrap();
            });
        }
    });

    // The ledger must remain valid JSONL: stats parses with no skipped entries
    // and no interleaved/corrupt lines from a shared temp file.
    let stats = stats_payload(&cfg, 0).unwrap();
    assert_eq!(
        stats["skipped_entries"].as_u64(),
        Some(0),
        "rotation produced corrupt/interleaved JSONL"
    );

    // The file is bounded near the cap (rename-race means one rotation wins; it
    // may include a few of the concurrently-appended lines).
    let contents = std::fs::read_to_string(&path).unwrap();
    let lines = contents.lines().count();
    assert!(
        lines <= MAX_HISTORY_ENTRIES + N,
        "ledger not bounded after concurrent rotations: {lines} lines"
    );
    assert!(
        lines >= MAX_HISTORY_ENTRIES - N,
        "ledger lost far too many entries: {lines} lines"
    );

    // No temp file orphaned by any rotation attempt.
    assert_eq!(
        orphan_tmp_count(dir.path()),
        0,
        "a rotation left an orphan .tmp file behind"
    );
}

#[test]
fn poison_line_does_not_wedge_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");

    // Build a file with enough valid lines to exceed the trigger, plus one line
    // of invalid UTF-8 that the line reader cannot decode. Pre-fix, this poison
    // line aborts every rotation and the ledger grows unbounded forever.
    {
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..(ROTATE_TRIGGER_ENTRIES + 10) {
            let entry = json!({
                "timestamp": "2024-01-01T00:00:00Z",
                "mode": "audio",
                "seq": i,
                "total_files": 0,
                "total_bytes": 0,
                "items": [],
            });
            f.write_all(serde_json::to_string(&entry).unwrap().as_bytes())
                .unwrap();
            f.write_all(b"\n").unwrap();
            // Inject the invalid-UTF-8 poison line early so it falls inside the
            // window that rotation must read through.
            if i == 5 {
                f.write_all(&[0xff, 0xfe, 0x80, b'\n']).unwrap();
            }
        }
    }

    let cfg = config_with_history(&path);
    // A single append triggers rotation; it must bound the file rather than wedge.
    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist P", true)).unwrap();

    let contents = std::fs::read(&path).unwrap();
    let line_count = contents.iter().filter(|&&b| b == b'\n').count();
    assert!(
        line_count <= MAX_HISTORY_ENTRIES + 1,
        "rotation wedged on poison line: {line_count} lines remain"
    );

    // The poison bytes were rotated OUT, so the trimmed ledger is now clean UTF-8
    // and parses without skips.
    let text = String::from_utf8(contents).expect("poison line was not dropped");
    let stats = stats_payload(&cfg, 0).unwrap();
    assert_eq!(stats["skipped_entries"].as_u64(), Some(0));
    assert!(text.contains("\"uploader\":\"Artist P\""));
}

#[test]
fn torn_partial_write_does_not_drop_the_valid_entry_glued_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("downloads.jsonl");

    // Reproduce a crash that left a torn partial write: a valid line, then raw
    // invalid-UTF-8 bytes terminated by `\n` (the poison fragment), then another
    // valid line on the very next physical line. With `BufRead::lines()` the
    // invalid-UTF-8 region could decode the poison fragment AND the valid line
    // after it as a single `Err`, dropping a real entry. The physical-line reader
    // (`read_until`) must drop ONLY the poison fragment and keep both neighbours.
    //
    // The torn region is placed near the END of the file so both bracketing
    // markers fall inside the retained `MAX_HISTORY_ENTRIES` tail after rotation.
    let total = ROTATE_TRIGGER_ENTRIES + 10;
    let poison_at = total - 3; // a few lines from the end
    let marker = |seq: usize| {
        let entry = json!({
            "timestamp": "2024-01-01T00:00:00Z",
            "mode": "audio",
            "seq": seq,
            "total_files": 0,
            "total_bytes": 0,
            "items": [],
        });
        serde_json::to_string(&entry).unwrap()
    };
    {
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..total {
            f.write_all(marker(i).as_bytes()).unwrap();
            f.write_all(b"\n").unwrap();
            // Immediately after the line at `poison_at`, splice a torn fragment:
            // invalid UTF-8 bytes + a newline, directly abutting the next valid
            // line (seq == poison_at + 1).
            if i == poison_at {
                f.write_all(&[0xff, 0xfe, 0x80, 0xc0, b'\n']).unwrap();
            }
        }
    }

    let cfg = config_with_history(&path);
    append_download(&cfg, DownloadMode::Audio, &sample_payload("Artist T", true)).unwrap();

    // The ledger is bounded and now clean UTF-8 (the poison fragment was dropped).
    let contents = std::fs::read(&path).unwrap();
    let line_count = contents.iter().filter(|&&b| b == b'\n').count();
    assert!(
        line_count <= MAX_HISTORY_ENTRIES + 1,
        "rotation did not bound the ledger: {line_count} lines"
    );
    let text = String::from_utf8(contents).expect("poison fragment was not dropped");

    // BOTH valid lines bracketing the poison survived: the one just before it and
    // the one glued immediately after it. The prior `lines()` behaviour would have
    // lost the latter along with the poison fragment.
    let before = format!("\"seq\":{}", poison_at);
    let after = format!("\"seq\":{}", poison_at + 1);
    assert!(
        text.contains(&before),
        "valid line before the poison fragment was dropped"
    );
    assert!(
        text.contains(&after),
        "valid line glued after the poison fragment was dropped"
    );

    // Stats parse the trimmed ledger cleanly and the appended record is present.
    let stats = stats_payload(&cfg, 0).unwrap();
    assert_eq!(stats["skipped_entries"].as_u64(), Some(0));
    assert!(text.contains("\"uploader\":\"Artist T\""));
}
