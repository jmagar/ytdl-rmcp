//! ytdl-mcp — a cross-platform MCP server that downloads media with yt-dlp,
//! tags it, and rsyncs it to an SSH remote.
//!
//! Bare invocation serves MCP over stdio. `ytdl-mcp setup` installs the
//! external tools and registers the server into the user's agent CLIs.

mod bootstrap;
mod config;
mod downloader;
mod mcp;
mod model;
mod service;
mod setup;
mod transfer;
mod urls;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};
use rmcp::{transport::stdio, ServiceExt};

use crate::config::Config;
use crate::mcp::YtdlServer;

#[derive(Parser)]
#[command(name = "ytdl-mcp", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Install yt-dlp + ffmpeg and register this server into your agent CLIs.
    Setup,
    /// Serve MCP over stdio (the default when no subcommand is given).
    Serve,
}

#[tokio::main]
async fn main() -> Result<()> {
    // ALL logging goes to stderr — stdout is the JSON-RPC channel.
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Some(Command::Setup) => setup::run().await,
        Some(Command::Serve) | None => serve().await,
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_env("YTDLP_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
}

async fn serve() -> Result<()> {
    let cfg = Config::from_env();
    tracing::info!("ytdl-mcp serving over stdio");
    let service = YtdlServer::new(cfg).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
