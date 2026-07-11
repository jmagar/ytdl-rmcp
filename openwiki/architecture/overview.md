# Architecture Overview

ytdl-mcp is a Rust-based MCP server built on the [`rmcp`](https://crates.io/crates/rmcp) crate, serving six tools over stdio transport. All source files stay under 500 LOC, with test files as siblings (`foo_tests.rs`).

## MCP server design

The server implements `rmcp::ServerHandler` via the `YtdlServer` struct in [`src/mcp.rs`](../../src/mcp.rs). Tools are declared with `#[tool]` macros and dispatched through `#[tool_router]`:

- **`youtube_download`** — Orchestrates download, metadata embedding, and SSH transfer (backed by [`service::run_download`](../../src/service.rs))
- **`youtube_search`** — Queries YouTube via yt-dlp and returns URLs (backed by [`service::run_search`](../../src/service.rs))
- **`youtube_search_ui`** — Serves an MCP App HTML resource for interactive search (backed by [`search_app.rs`](../../src/search_app.rs))
- **`youtube_probe`** — Resolves metadata without downloading media (backed by [`service::run_probe`](../../src/service.rs))
- **`youtube_identify`** — Fingerprints local audio and returns MusicBrainz candidates (backed by [`service::run_identify`](../../src/service.rs))
- **`youtube_stats`** — Aggregates the JSONL download history ledger (backed by [`service::run_stats`](../../src/history.rs))

## Module layout

| File | Role |
| --- | --- |
| [`main.rs`](../../src/main.rs) | CLI dispatch (`serve`, `setup`, `doctor`) and stderr tracing |
| [`config.rs`](../../src/config.rs) | `Config::from_env_result` — all `YTDLP_*` env var parsing |
| [`doctor.rs`](../../src/doctor.rs) | Read-only diagnostics: version, git SHA, platform, tool paths |
| [`model.rs`](../../src/model.rs) | Tool input structs with serde + schemars validation |
| [`mcp.rs`](../../src/mcp.rs) | `rmcp` server handler and tool router |
| [`service.rs`](../../src/service.rs) | High-level orchestration: tools → download → transfer → response |
| [`service/format.rs`](../../src/service/format.rs) | Response rendering (JSON/Markdown) and `DownloadPayload` |
| [`service/plex_tracks.rs`](../../src/service/plex_tracks.rs) | Plex playlist track matching logic |
| [`service/retag.rs`](../../src/service/retag.rs) | AcoustID auto-retagging for downloaded audio |
| [`downloader.rs`](../../src/downloader.rs) | yt-dlp subprocess runner, output parsing, and `fetch` orchestration |
| [`downloader/probe.rs`](../../src/downloader/probe.rs) | Metadata-only yt-dlp queries (no media download) |
| [`transfer.rs`](../../src/transfer.rs) | rsync/scp subprocess wrapper with `ensure_remote_dir` |
| [`history.rs`](../../src/history.rs) | JSONL download ledger with rotation and `youtube_stats` aggregation |
| [`identify.rs`](../../src/identify.rs) | AcoustID fingerprint (fpcalc) → MusicBrainz lookup → retag preview |
| [`identify/musicbrainz.rs`](../../src/identify/musicbrainz.rs) | MusicBrainz REST client and candidate scoring |
| [`identify/tagger.rs`](../../src/identify/tagger.rs) | Writes preview tags into audio files via `lofty` |
| [`plex.rs`](../../src/plex.rs) | Optional Plex playlist sync (match + add downloaded tracks) |
| [`search_app.rs`](../../src/search_app.rs) | MCP App HTML resource backing `youtube_search_ui` |
| [`bootstrap.rs`](../../src/bootstrap.rs) | Tool resolution (env → PATH → cache → download) |
| [`bootstrap/ytdlp.rs`](../../src/bootstrap/ytdlp.rs) | yt-dlp auto-download with SHA256 pinning |
| [`bootstrap/ffmpeg.rs`](../../src/bootstrap/ffmpeg.rs) | ffmpeg auto-download via `ffmpeg-sidecar` |
| [`bootstrap/http.rs`](../../src/bootstrap/http.rs) | Shared HTTP client for bootstrap downloads |
| [`urls.rs`](../../src/urls.rs) | YouTube mix/radio URL cleaning |
| [`setup.rs`](../../src/setup.rs) | Interactive installer registering into claude/codex/gemini |
| [`util.rs`](../../src/util.rs) | Shared subprocess runner (`run_capped`) and error helpers |

Tests are sibling files (`foo_tests.rs`) wired via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`.

## Conventions

- **500 LOC limit** — Split larger files into `foo/` submodules instead
- **No `mod.rs`** — Use `foo.rs` with `mod bar;` resolving to `foo/bar.rs`
- **Sibling test files** — Never inline `mod tests {}`; use `foo_tests.rs`
- **Stdout is JSON-RPC** — All logging goes to stderr; never print to stdout
- **Blocking operations** — File I/O and subprocess runs use `spawn_blocking` to avoid blocking the async runtime

## Tool resolution and caching

External tools (yt-dlp, ffmpeg, rsync, scp, ssh, fpcalc) are resolved through [`bootstrap.rs`](../../src/bootstrap.rs):

1. Check env override (`YTDLP_PATH`, `FFMPEG_PATH`, `FPCALC_PATH`)
2. Search `PATH` with `which`
3. Fall back to per-user cache download (yt-dlp/ffmpeg only)
4. Optional SHA256 pinning via `YTDLP_SHA256`/`FFMPEG_SHA256`

Resolved tools are cached per-process in `service::ToolsCache` to avoid repeated bootstrap overhead on each tool call.

## Security model

- **URL validation** — Tool-call URLs are validated as `http`/`https` and passed after a `--` end-of-options separator
- **Path validation** — `RemoteSpec` and `RemotePath` reject empty, option-like, and whitespace/control-character values
- **Non-interactive SSH** — `BatchMode=yes` and `StrictHostKeyChecking=accept-new` prevent hanging on prompts
- **No stdout pollution** — yt-dlp stdout is captured and parsed; all logging goes to stderr
