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
    let one: Urls = serde_json::from_str(r#""https://x/v=1""#).unwrap();
    assert_eq!(one.into_vec(), vec!["https://x/v=1"]);

    let many: Urls = serde_json::from_str(r#"["a","b"]"#).unwrap();
    assert_eq!(many.into_vec(), vec!["a", "b"]);
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
    assert_eq!(input.urls.into_vec(), vec!["https://x"]);
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
fn enum_strings_match_cli_values() {
    assert_eq!(AudioFormat::M4a.as_str(), "m4a");
    assert_eq!(VideoContainer::Mkv.as_str(), "mkv");
}
