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
use crate::model::{DownloadInput, IdentifyInput, ProbeInput, SearchInput, StatsInput};
use crate::search_app;
use crate::service;

/// Build a uniform error result: `CallToolResult::error` with the `"Error: {e}"`
/// content shape. All success/error helpers funnel their error path through this.
fn error_tool_result(error: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![Content::text(format!("Error: {error}"))])
}

/// Wrap a fallible text-producing operation in the shared tool-result contract:
/// `Ok(text)` → a success result, `Err(e)` → [`error_tool_result`].
fn text_tool_result<E: std::fmt::Display>(
    result: std::result::Result<String, E>,
) -> CallToolResult {
    match result {
        Ok(text) => CallToolResult::success(vec![Content::text(text)]),
        Err(e) => error_tool_result(e),
    }
}

/// Like [`text_tool_result`], but for a structured payload backing an MCP App:
/// the success result carries the pretty-printed JSON as text content plus
/// `structured_content` and the supplied `meta` (the App resource pointer). The
/// error path is identical to [`text_tool_result`] / [`error_tool_result`].
fn structured_tool_result<T, E>(
    result: std::result::Result<T, E>,
    meta: rmcp::model::Meta,
) -> CallToolResult
where
    T: serde::Serialize,
    E: std::fmt::Display,
{
    match result {
        Ok(payload) => {
            let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".into());
            let mut result = CallToolResult::success(vec![Content::text(text)]);
            result.structured_content = Some(serde_json::to_value(&payload).unwrap_or_default());
            result.meta = Some(meta);
            result
        }
        Err(e) => error_tool_result(e),
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;

#[derive(Clone)]
pub struct YtdlServer {
    cfg: Arc<Config>,
    /// Process-lifetime cache of the resolved external tools (yt-dlp + ffmpeg).
    /// Shared across clones so bootstrap resolution + its exclusive cross-process
    /// lock run once per process instead of on every tool call. See
    /// [`service::ToolsCache`].
    tools: Arc<service::ToolsCache>,
    // Referenced by the #[tool_handler] expansion; kept even though direct reads
    // aren't visible to dead-code analysis.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl YtdlServer {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Arc::new(cfg),
            tools: Arc::new(service::ToolsCache::default()),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl YtdlServer {
    /// Download audio, video, or both from one or more URLs with yt-dlp, embed
    /// metadata + cover art, organize by artist, and transfer the result to a
    /// target path. Audio and video can go to separate destinations.
    #[tool(
        name = "youtube_download",
        description = "Download audio/video from a yt-dlp-supported URL, tag it, and transfer it to a local, SSH, or rclone target."
    )]
    async fn youtube_download(
        &self,
        Parameters(input): Parameters<DownloadInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let started = std::time::Instant::now();
        tracing::info!(
            service = "ytdl-rmcp",
            tool = "youtube_download",
            mode = ?input.mode,
            "tool dispatch start"
        );
        let result = service::run_download(&self.cfg, &self.tools, input).await;
        let elapsed_ms = started.elapsed().as_millis();
        match &result {
            Ok(_) => tracing::info!(
                service = "ytdl-rmcp",
                tool = "youtube_download",
                elapsed_ms,
                "tool dispatch success"
            ),
            Err(e) => {
                tracing::warn!(service = "ytdl-rmcp", tool = "youtube_download", elapsed_ms, error = %e, "tool dispatch error")
            }
        }
        Ok(text_tool_result(result))
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
        let started = std::time::Instant::now();
        tracing::info!(
            service = "ytdl-rmcp",
            tool = "youtube_probe",
            "tool dispatch start"
        );
        let result = service::run_probe(&self.cfg, &self.tools, input).await;
        let elapsed_ms = started.elapsed().as_millis();
        match &result {
            Ok(_) => tracing::info!(
                service = "ytdl-rmcp",
                tool = "youtube_probe",
                elapsed_ms,
                "tool dispatch success"
            ),
            Err(e) => {
                tracing::warn!(service = "ytdl-rmcp", tool = "youtube_probe", elapsed_ms, error = %e, "tool dispatch error")
            }
        }
        Ok(text_tool_result(result))
    }

    /// Fingerprint local audio files with Chromaprint/fpcalc and return
    /// AcoustID/MusicBrainz candidates, with optional tag writing.
    #[tool(
        name = "youtube_identify",
        description = "Identify local audio files with fpcalc + AcoustID, return MusicBrainz recording candidates, and optionally write high-confidence canonical tags."
    )]
    async fn youtube_identify(
        &self,
        Parameters(input): Parameters<IdentifyInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let started = std::time::Instant::now();
        tracing::info!(
            service = "ytdl-rmcp",
            tool = "youtube_identify",
            "tool dispatch start"
        );
        let result = service::run_identify(&self.cfg, input).await;
        let elapsed_ms = started.elapsed().as_millis();
        match &result {
            Ok(_) => tracing::info!(
                service = "ytdl-rmcp",
                tool = "youtube_identify",
                elapsed_ms,
                "tool dispatch success"
            ),
            Err(e) => {
                tracing::warn!(service = "ytdl-rmcp", tool = "youtube_identify", elapsed_ms, error = %e, "tool dispatch error")
            }
        }
        Ok(text_tool_result(result))
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
        let started = std::time::Instant::now();
        tracing::info!(service = "ytdl-rmcp", tool = "youtube_search", query = %input.query, "tool dispatch start");
        let result = service::run_search(&self.cfg, &self.tools, input).await;
        let elapsed_ms = started.elapsed().as_millis();
        match &result {
            Ok(_) => tracing::info!(
                service = "ytdl-rmcp",
                tool = "youtube_search",
                elapsed_ms,
                "tool dispatch success"
            ),
            Err(e) => {
                tracing::warn!(service = "ytdl-rmcp", tool = "youtube_search", elapsed_ms, error = %e, "tool dispatch error")
            }
        }
        Ok(text_tool_result(result))
    }

    /// Summarize the persistent download ledger written by `youtube_download`.
    #[tool(
        name = "youtube_stats",
        description = "Summarize ytdl-rmcp download history, totals, file kinds, uploaders, and recent entries."
    )]
    async fn youtube_stats(
        &self,
        Parameters(input): Parameters<StatsInput>,
    ) -> Result<CallToolResult, ErrorData> {
        tracing::info!(
            service = "ytdl-rmcp",
            tool = "youtube_stats",
            "tool dispatch start"
        );
        let result = service::run_stats(&self.cfg, input);
        match &result {
            Ok(_) => tracing::info!(
                service = "ytdl-rmcp",
                tool = "youtube_stats",
                "tool dispatch success"
            ),
            Err(e) => {
                tracing::warn!(service = "ytdl-rmcp", tool = "youtube_stats", error = %e, "tool dispatch error")
            }
        }
        Ok(text_tool_result(result))
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
        let started = std::time::Instant::now();
        tracing::info!(service = "ytdl-rmcp", tool = "youtube_search_ui", query = %input.query, "tool dispatch start");
        let result = service::run_search_payload(&self.cfg, &self.tools, &input).await;
        let elapsed_ms = started.elapsed().as_millis();
        match &result {
            Ok(payload) => tracing::info!(
                service = "ytdl-rmcp",
                tool = "youtube_search_ui",
                elapsed_ms,
                result_count = payload.results.len(),
                "tool dispatch success"
            ),
            Err(e) => {
                tracing::warn!(service = "ytdl-rmcp", tool = "youtube_search_ui", elapsed_ms, error = %e, "tool dispatch error")
            }
        }
        Ok(structured_tool_result(result, search_app::tool_meta()))
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
        .with_server_info(Implementation::new(
            "ytdl-rmcp",
            concat!(env!("CARGO_PKG_VERSION"), " (", env!("YTDL_GIT_SHA"), ")"),
        ))
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
