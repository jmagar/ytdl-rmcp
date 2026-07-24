---
type: Reference
title: "ytdl-mcp Quickstart"
description: "Entry point for getting started with ytdl-mcp and navigating to architecture, operations, and workflows."
---

# ytdl-mcp Quickstart

**ytdl-mcp** is a cross-platform, single-binary MCP (Model Context Protocol) server that downloads media from YouTube and other yt-dlp-supported sites, embeds metadata and cover art, organizes files by artist, and transfers results to an SSH remote.

## What this project does

ytdl-mcp provides eight MCP tools that integrate media workflows into AI coding assistants:

- **`youtube_download`** — Download audio, video, or both from URLs, tag them with metadata, and transfer results to a local, SSH, or rclone target
- **`youtube_search`** — Search YouTube with yt-dlp and return result URLs without downloading
- **`youtube_search_ui`** — Open an interactive YouTube search UI (MCP App) for selecting videos
- **`youtube_probe`** — Resolve title, duration, uploader, and format counts without downloading media
- **`youtube_identify`** — Fingerprint local audio files with AcoustID/MusicBrainz and preview or write canonical tags
- **`youtube_stats`** — Summarize the JSONL download history ledger (totals, file kinds, uploaders, recent entries)
- **`youtube_plex_playlist`** — Build or preview Plex playlists from successful audio download history
- **`youtube_transfer_queue`** — List and drain retained-staging transfer failure manifests


The server runs over stdio as an MCP server, auto-downloads yt-dlp and ffmpeg into a per-user cache, and supports both bare binary and containerized deployment.

## Start here

- **[Architecture overview](architecture/overview.md)** — MCP server design, tool surface, and module layout
- **[Download workflow](workflows/download-flow.md)** — How `youtube_download` resolves tools, downloads media, embeds tags, and transfers to remotes
- **[Setup and configuration](operations/setup.md)** — Environment variables, installation modes, and distribution channels
- **[Container runtime](operations/container.md)** — Docker image usage, SSH credentials, and volume mounts
- **[Build, test, and cross-compilation](development/build-test.md)** — Development workflow, CI gates, and release process

## Key concepts

- **Tools** — Eight MCP tools (including Plex and transfer queue operations) backed by a single Rust binary on stdio transport
- **Bootstrap** — Runtime auto-download of yt-dlp and ffmpeg with optional SHA256 pinning
- **Transfer** — rsync with scp fallback, non-interactive SSH, separate audio/video destinations
- **Metadata** — Embedded tags (title/artist/album/date) with cover art, `Artist/Title [id]` file organization
- **History** — JSONL download ledger with `youtube_stats` aggregation
- **Identification** — AcoustID fingerprinting → MusicBrainz lookup → retag preview/write

## Working on this repository

When making code changes, read these pages first:

- **[Build, test, and cross-compilation](development/build-test.md)** — Run tests, clippy, and cross-builds before committing
- **[Download workflow](workflows/download-flow.md)** — Understand the download/transfer orchestration path
- **[Architecture overview](architecture/overview.md)** — Module boundaries and conventions (500 LOC limit, sibling test files)

When modifying workflows, product behavior, or tool surfaces, update the relevant workflow/architecture pages to keep the wiki accurate.

## Distribution

ytdl-mcp is distributed through multiple channels:

- **GitHub releases** — Linux and Windows MSVC binaries attached to `v*` releases
- **Claude Code plugin** — Available in the `jmagar/lab` marketplace as `ytdl-mcp`
- **Gemini extension** — `gemini-extension.json` with `YTDLP_*` env var mappings
- **MCP bundle** — `.mcpb`/`.dxt` package targeting `["linux", "win32"]`

See [Setup and configuration](operations/setup.md) for installation instructions per channel.
