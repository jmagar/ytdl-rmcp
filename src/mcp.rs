//! The rmcp `ServerHandler` — advertises and dispatches the MCP tools.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler};

use crate::config::Config;
use crate::model::{DownloadInput, ProbeInput, SearchInput};
use crate::search_app;
use crate::service;

fn text_tool_result<E: std::fmt::Display>(
    result: std::result::Result<String, E>,
) -> Result<CallToolResult, ErrorData> {
    Ok(match result {
        Ok(text) => CallToolResult::success(vec![Content::text(text)]),
        Err(e) => error_tool_result(e),
    })
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;

fn error_tool_result(error: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![Content::text(format!("Error: {error}"))])
}

#[derive(Clone)]
pub struct YtdlServer {
    cfg: Arc<Config>,
    // Referenced by the #[tool_handler] expansion; kept even though direct reads
    // aren't visible to dead-code analysis.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl YtdlServer {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Arc::new(cfg),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl YtdlServer {
    /// Download audio, video, or both from one or more URLs with yt-dlp, embed
    /// metadata + cover art, organize by artist, and rsync the result to a
    /// directory on an SSH remote. Audio and video go to separate destinations.
    #[tool(
        name = "youtube_download",
        description = "Download audio/video from a yt-dlp-supported URL, tag it, and rsync to an SSH remote."
    )]
    async fn youtube_download(
        &self,
        Parameters(input): Parameters<DownloadInput>,
    ) -> Result<CallToolResult, ErrorData> {
        text_tool_result(service::run_download(&self.cfg, input).await)
    }

    /// Resolve title/duration/uploader/format counts for URLs without
    /// downloading. Useful to confirm a target before a large download.
    #[tool(
        name = "youtube_probe",
        description = "Inspect media metadata (title, duration, uploader, formats) without downloading."
    )]
    async fn youtube_probe(
        &self,
        Parameters(input): Parameters<ProbeInput>,
    ) -> Result<CallToolResult, ErrorData> {
        text_tool_result(service::run_probe(&self.cfg, input).await)
    }

    /// Search YouTube through yt-dlp without downloading. Returns result URLs that
    /// can be passed to `youtube_probe` or `youtube_download`.
    #[tool(
        name = "youtube_search",
        description = "Search YouTube with yt-dlp and return matching video URLs without downloading."
    )]
    async fn youtube_search(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, ErrorData> {
        text_tool_result(service::run_search(&self.cfg, input).await)
    }

    /// Open the interactive YouTube search MCP App. UI-capable hosts render the
    /// embedded Aurora search panel; other hosts receive text fallback results.
    #[tool(
        name = "youtube_search_ui",
        description = "Open an interactive YouTube search UI for selecting videos to probe or download.",
        meta = search_app::tool_meta(),
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::model::SearchPayload>()
    )]
    async fn youtube_search_ui(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, ErrorData> {
        match service::run_search_payload(&self.cfg, &input).await {
            Ok(payload) => {
                let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into());
                let mut result = CallToolResult::success(vec![Content::text(text)]);
                result.structured_content =
                    Some(serde_json::to_value(&payload).unwrap_or_default());
                result.meta = Some(search_app::tool_meta());
                Ok(result)
            }
            Err(e) => Ok(error_tool_result(e)),
        }
    }
}

#[tool_handler]
impl ServerHandler for YtdlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("ytdl-mcp", env!("CARGO_PKG_VERSION")))
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(search_app::list_app_resources()))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, ErrorData>> + Send + '_ {
        std::future::ready(search_app::read_app_resource(&request.uri).ok_or_else(|| {
            ErrorData::invalid_params(format!("Unknown resource URI: {}", request.uri), None)
        }))
    }
}
