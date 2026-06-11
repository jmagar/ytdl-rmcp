use rmcp::model::ResourceContents;

#[test]
fn app_resource_uri_is_stable() {
    assert_eq!(super::RESOURCE_URI, "ui://ytdl-mcp/youtube-search.html");
}

#[test]
fn app_resource_contains_html_and_aurora_hooks() {
    let result = super::read_app_resource(super::RESOURCE_URI).unwrap();
    let ResourceContents::TextResourceContents {
        text,
        mime_type,
        meta,
        ..
    } = &result.contents[0]
    else {
        panic!("expected text resource");
    };

    assert_eq!(mime_type.as_deref(), Some(super::RESOURCE_MIME_TYPE));
    assert!(text.contains("YouTube search"));
    assert!(text.contains("--aurora-page-bg"));
    assert!(text.contains("callServerTool"));
    assert!(text.contains("window.McpExtApps"));
    assert!(!text.contains("{{MCP_EXT_APPS_BUNDLE}}"));
    assert!(!text.contains("https://esm.sh"));
    let ui = meta.as_ref().unwrap().0.get("ui").unwrap();
    assert_eq!(
        ui["csp"]["resourceDomains"][0],
        serde_json::json!("https://i.ytimg.com")
    );
}

#[test]
fn tool_meta_links_to_app_resource() {
    let meta = super::tool_meta();

    assert_eq!(
        meta.0["ui"]["resourceUri"],
        serde_json::json!(super::RESOURCE_URI)
    );
}
