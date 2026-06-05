//! The rmcp `ServerHandler` — advertises and dispatches the two tools.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};

use crate::config::Config;
use crate::model::{DownloadInput, ProbeInput};
use crate::service;

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
        match service::run_download(&self.cfg, input).await {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {e}"
            ))])),
        }
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
        match service::run_probe(&self.cfg, input).await {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {e}"
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for YtdlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("ytdl-mcp", env!("CARGO_PKG_VERSION")))
    }
}
