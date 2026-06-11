#[test]
fn youtube_search_ui_advertises_app_metadata_and_output_schema() {
    let tools = super::YtdlServer::tool_router().list_all();
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
