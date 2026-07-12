use super::*;

#[test]
fn audio_format_parse_defaults_to_mp3() {
    assert_eq!(AudioFormat::parse_or_default("opus"), AudioFormat::Opus);
    assert_eq!(AudioFormat::parse_or_default("FLAC"), AudioFormat::Flac);
    assert_eq!(AudioFormat::parse_or_default(" m4a "), AudioFormat::M4a);
    assert_eq!(AudioFormat::parse_or_default("nonsense"), AudioFormat::Mp3);
    assert_eq!(AudioFormat::parse_or_default(""), AudioFormat::Mp3);
}

#[test]
fn audio_format_properties() {
    assert!(AudioFormat::Wav.is_lossless_or_passthrough());
    assert!(AudioFormat::Flac.is_lossless_or_passthrough());
    assert!(AudioFormat::Best.is_lossless_or_passthrough());
    assert!(!AudioFormat::Mp3.is_lossless_or_passthrough());

    assert!(AudioFormat::Mp3.is_taggable());
    assert!(AudioFormat::Opus.is_taggable());
    assert!(!AudioFormat::Wav.is_taggable()); // WAV has no usable tag/cover support
}

#[test]
fn urls_accepts_single_string_or_array() {
    // `Urls` deserializes the string-or-array shape and only exposes the
    // validating extractor; both forms round-trip through it.
    let one: Urls = serde_json::from_str(r#""https://x/v=1""#).unwrap();
    assert_eq!(one.into_validated_vec().unwrap(), vec!["https://x/v=1"]);

    let many: Urls = serde_json::from_str(r#"["https://a","https://b"]"#).unwrap();
    assert_eq!(
        many.into_validated_vec().unwrap(),
        vec!["https://a", "https://b"]
    );
}

#[test]
fn urls_into_validated_vec_accepts_http_and_https() {
    let one: Urls = serde_json::from_str(r#""https://www.youtube.com/watch?v=1""#).unwrap();
    assert_eq!(
        one.into_validated_vec().unwrap(),
        vec!["https://www.youtube.com/watch?v=1"]
    );

    let many: Urls =
        serde_json::from_str(r#"["http://example.com/a","https://example.com/b"]"#).unwrap();
    assert_eq!(
        many.into_validated_vec().unwrap(),
        vec!["http://example.com/a", "https://example.com/b"]
    );
}

#[test]
fn urls_into_validated_vec_rejects_non_http_and_flaglike_values() {
    // A value that yt-dlp would otherwise parse as a flag (argument injection).
    let exec: Urls = serde_json::from_str(r#""--exec=touch /tmp/pwned""#).unwrap();
    let err = exec.into_validated_vec().unwrap_err().to_string();
    assert!(err.contains("only http:// and https://"));

    // Non-http(s) schemes are rejected too.
    assert!(Urls(OneOrMany::One("file:///etc/passwd".into()))
        .into_validated_vec()
        .is_err());
    assert!(Urls(OneOrMany::One("-o/tmp/x".into()))
        .into_validated_vec()
        .is_err());

    // One bad entry in a list fails the whole batch.
    let mixed: Urls = serde_json::from_str(r#"["https://ok.example/v","--exec=bad"]"#).unwrap();
    assert!(mixed.into_validated_vec().is_err());
}

#[test]
fn urls_into_validated_vec_rejects_embedded_control_characters() {
    // SEC-F2: a value that passes the scheme + host check but smuggles an interior
    // control character must be rejected, or it would survive trimming and inject
    // forged lines into the JSONL ledger / reflected error messages.
    for evil in [
        "https://example.com/\nX: y",       // embedded newline
        "https://example.com/\rX-Injected", // embedded carriage return
        "https://example.com/\u{0}null",    // embedded NUL
    ] {
        let bad = Urls(OneOrMany::One(evil.into()));
        let err = bad.into_validated_vec().unwrap_err().to_string();
        assert!(
            err.contains("control characters"),
            "expected control-char rejection for {evil:?}, got: {err}"
        );
    }
}

#[test]
fn urls_into_validated_vec_trims_surrounding_whitespace() {
    // CR-2: a leading-space URL validates but the trimmed form is what reaches
    // yt-dlp, so the space can't be misparsed downstream.
    let padded: Urls = serde_json::from_str(r#""  https://example.com/v=1  ""#).unwrap();
    assert_eq!(
        padded.into_validated_vec().unwrap(),
        vec!["https://example.com/v=1"]
    );
}

#[test]
fn max_search_limit_clamps_at_boundaries() {
    assert_eq!(MAX_SEARCH_LIMIT, 25);
    let over: SearchInput = serde_json::from_str(r#"{"query":"x","limit":26}"#).unwrap();
    let under: SearchInput = serde_json::from_str(r#"{"query":"x","limit":0}"#).unwrap();
    assert_eq!(over.effective_limit(), MAX_SEARCH_LIMIT);
    assert_eq!(under.effective_limit(), 1);
}

#[test]
fn download_input_applies_defaults() {
    // Only `urls` is required; everything else defaults.
    let input: DownloadInput = serde_json::from_str(r#"{"urls":"https://x"}"#).unwrap();
    assert_eq!(input.mode, DownloadMode::Audio);
    assert_eq!(input.audio_format, None); // resolved from config at call time
    assert_eq!(input.audio_quality, "0");
    assert_eq!(input.container, VideoContainer::Mp4);
    assert!(input.max_height.is_none());
    assert!(!input.keep_local);
    assert!(!input.use_archive);
    assert_eq!(input.response_format, ResponseFormat::Markdown);
    assert_eq!(input.urls.into_validated_vec().unwrap(), vec!["https://x"]);
}

#[test]
fn download_input_honors_explicit_fields() {
    let input: DownloadInput = serde_json::from_str(
        r#"{"urls":["u"],"mode":"both","audio_format":"flac","max_height":1080,"response_format":"json"}"#,
    )
    .unwrap();
    assert_eq!(input.mode, DownloadMode::Both);
    assert_eq!(input.audio_format, Some(AudioFormat::Flac));
    assert_eq!(input.max_height, Some(1080));
    assert_eq!(input.response_format, ResponseFormat::Json);
}

#[test]
fn identify_input_accepts_single_path_or_array() {
    let one: IdentifyInput = serde_json::from_str(r#"{"paths":"/tmp/song.mp3"}"#).unwrap();
    assert_eq!(one.paths.into_vec(), vec!["/tmp/song.mp3"]);
    assert_eq!(one.response_format, ResponseFormat::Markdown);
    assert!(!one.write_tags);

    let many: IdentifyInput = serde_json::from_str(
        r#"{"paths":["/tmp/a.mp3","/tmp/b.m4a"],"write_tags":true,"response_format":"json"}"#,
    )
    .unwrap();
    assert_eq!(many.paths.into_vec(), vec!["/tmp/a.mp3", "/tmp/b.m4a"]);
    assert_eq!(many.response_format, ResponseFormat::Json);
    assert!(many.write_tags);
}

#[test]
fn search_input_defaults_limit_and_markdown() {
    let input: SearchInput = serde_json::from_str(r#"{"query":"slow pulp live"}"#).unwrap();

    assert_eq!(input.query, "slow pulp live");
    assert_eq!(input.limit, 10);
    assert_eq!(input.response_format, ResponseFormat::Markdown);
}

#[test]
fn search_input_clamps_limit_to_supported_range() {
    let low: SearchInput = serde_json::from_str(r#"{"query":"x","limit":0}"#).unwrap();
    let high: SearchInput = serde_json::from_str(r#"{"query":"x","limit":100}"#).unwrap();

    assert_eq!(low.effective_limit(), 1);
    assert_eq!(high.effective_limit(), 25);
}

#[test]
fn plex_playlist_input_preserves_zero_and_caps_positive_limits() {
    let unlimited: PlexPlaylistInput = serde_json::from_str(r#"{"limit":0}"#).unwrap();
    let high: PlexPlaylistInput = serde_json::from_str(r#"{"limit":9999}"#).unwrap();

    assert_eq!(unlimited.effective_limit(), 0);
    assert_eq!(high.effective_limit(), MAX_PLAYLIST_LIMIT as usize);
}

#[test]
fn enum_strings_match_cli_values() {
    assert_eq!(AudioFormat::M4a.as_str(), "m4a");
    assert_eq!(VideoContainer::Mkv.as_str(), "mkv");
}
