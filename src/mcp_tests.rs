use rmcp::ServerHandler;

use super::{error_tool_result, structured_tool_result, text_tool_result, YtdlServer};
use crate::config::Config;

/// A minimal, valid `Config` built with benign defaults. These router/get_info
/// tests only need a constructed `YtdlServer`; they must NOT read process env
/// (`Config::from_env`), which is unguarded by the env-lock used elsewhere and
/// would flake if any `YTDLP_*` var is set or another test mutates env.
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
        auto_update: true,
        max_age_days: 14,
        update_pre: false,
        ytdlp_path: None,
        ffmpeg_path: None,
        extractor_args: None,
        ytdlp_sha256: None,
        ffmpeg_sha256: None,
        ytdlp_timeout_secs: 1800,
        transfer_timeout_secs: 600,
    }
}

/// Every tool this server exposes. The dispatch surface is the source of truth
/// for the MCP contract, so the test pins the exact set.
const EXPECTED_TOOLS: [&str; 6] = [
    "youtube_download",
    "youtube_probe",
    "youtube_identify",
    "youtube_search",
    "youtube_stats",
    "youtube_search_ui",
];

#[test]
fn youtube_search_ui_advertises_app_metadata_and_output_schema() {
    let tools = YtdlServer::tool_router().list_all();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "youtube_search_ui")
        .expect("youtube_search_ui tool");

    assert_eq!(
        tool.meta.as_ref().unwrap().0["ui"]["resourceUri"],
        serde_json::json!(super::search_app::RESOURCE_URI)
    );
    assert!(
        tool.output_schema.is_some(),
        "youtube_search_ui should advertise SearchPayload structured output"
    );
}

#[test]
fn tool_router_advertises_all_six_tools() {
    let tools = YtdlServer::tool_router().list_all();
    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();

    let mut expected: Vec<&str> = EXPECTED_TOOLS.to_vec();
    expected.sort_unstable();

    assert_eq!(
        names, expected,
        "the dispatch surface must advertise exactly the six MCP tools"
    );

    // Every advertised tool must carry an input schema so a client can call it.
    for tool in &tools {
        assert!(
            !tool.input_schema.is_empty(),
            "{} should advertise an input schema",
            tool.name
        );
    }
}

#[test]
fn constructed_server_router_carries_every_tool_route() {
    // Build a real server instance and confirm its per-instance tool router
    // (the one #[tool_handler] dispatches through) has a route for each tool.
    let server = YtdlServer::new(test_config());
    // `mcp_tests` is a child module of `mcp`, so the private `tool_router` field
    // on the constructed instance is reachable here.
    for name in EXPECTED_TOOLS {
        assert!(
            server.tool_router.has_route(name),
            "constructed server router is missing a route for {name}"
        );
    }
}

#[test]
fn get_info_enables_tools_and_resources() {
    let server = YtdlServer::new(test_config());
    let info = server.get_info();

    assert!(
        info.capabilities.tools.is_some(),
        "server must advertise tool capability"
    );
    assert!(
        info.capabilities.resources.is_some(),
        "server must advertise resource capability (search-UI app resource)"
    );
    assert_eq!(info.server_info.name, "ytdl-rmcp");
}

#[test]
fn text_tool_result_maps_ok_to_success_and_err_to_error() {
    // Ok -> success result carrying the text, is_error not set.
    let ok = text_tool_result::<std::io::Error>(Ok("hello world".to_string()));
    assert_ne!(ok.is_error, Some(true), "Ok should not be an error result");
    let text = ok.content[0]
        .as_text()
        .expect("success content should be text");
    assert_eq!(text.text, "hello world");

    // Err -> error result with the shared "Error: {e}" content shape.
    let err = text_tool_result::<&str>(Err("boom"));
    assert_eq!(err.is_error, Some(true), "Err should be an error result");
    let text = err.content[0]
        .as_text()
        .expect("error content should be text");
    assert_eq!(text.text, "Error: boom");
}

#[test]
fn structured_tool_result_carries_structured_content_meta_and_errors_gracefully() {
    let meta = super::search_app::tool_meta();

    // Ok -> success with structured_content + the app-resource meta attached.
    let ok = structured_tool_result::<_, &str>(
        Ok(serde_json::json!({ "query": "pulp", "results": [] })),
        meta.clone(),
    );
    assert_ne!(ok.is_error, Some(true));
    assert_eq!(
        ok.structured_content
            .as_ref()
            .expect("structured success must carry structured_content")["query"],
        "pulp"
    );
    assert!(
        ok.meta.is_some(),
        "structured success must carry the app-resource meta pointer"
    );

    // Err -> graceful error result, not a panic, no structured content.
    let err = structured_tool_result::<serde_json::Value, &str>(Err("bad params"), meta);
    assert_eq!(err.is_error, Some(true));
    assert!(err.structured_content.is_none());
    let text = err.content[0]
        .as_text()
        .expect("error content should be text");
    assert_eq!(text.text, "Error: bad params");
}

#[test]
fn error_tool_result_uses_the_shared_error_shape() {
    let result = error_tool_result("disk full");
    assert_eq!(result.is_error, Some(true));
    let text = result.content[0]
        .as_text()
        .expect("error content should be text");
    assert_eq!(text.text, "Error: disk full");
}
