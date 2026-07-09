# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- Validate tool-call URLs as `http`/`https` and pass every yt-dlp positional
  after a `--` end-of-options separator, so a `-`-prefixed value can't be parsed
  as a flag.
- Harden `RemotePath` against traversal for validated remote specs.

### Added

- `ytdl-rmcp doctor` subcommand: a read-only diagnostic report (version, git SHA,
  platform, resolved tool paths, and redacted config presence) for triaging a
  broken install.
- Embed the build's git SHA in `server_info`.

### Changed

- Replace the download payload with a typed `DownloadPayload` plus a
  `DownloadStatus` enum, and the `urls` input with a validated `Urls` newtype.
- Offload `youtube_identify` fingerprinting/lookups off the reactor so they do
  not block the async runtime.
- Rotate the JSONL history ledger with file locking.

### CI / Supply chain

- Pin GitHub Actions to commit SHAs and add `cargo-audit`, CodeQL, build
  provenance, and a Windows cross-build smoke test.
- Scan the container image with Trivy and pin its base image by digest.

## [0.7.0] - 2026-06-15

### Added

- **Six MCP tools** — `youtube_download`, `youtube_probe`, `youtube_identify`,
  `youtube_search`, `youtube_stats`, and `youtube_search_ui`.
- **Audio identification and auto-retag** via AcoustID + MusicBrainz. Local audio
  is fingerprinted with Chromaprint `fpcalc`, matched against AcoustID/MusicBrainz
  recording candidates, and high-confidence matches yield canonical
  artist/title/release/date/type/track metadata plus MusicBrainz IDs. Tags are
  written with `lofty`. `youtube_download` runs high-confidence retagging
  automatically in-place (before transfer) when `YTDLP_ACOUSTID_CLIENT_KEY` is
  set; `youtube_identify` previews by default and writes with `write_tags=true`.
- **Plex playlist sync** — when Plex credentials are configured, successful
  downloads that produced audio are added to a Plex playlist (defaulting to
  `yt-dlp Downloads`), creating it if needed and skipping tracks already present.
  Plex failures are reported without failing the download.
- **JSONL download history + `youtube_stats` aggregation** — every completed
  download call appends a ledger entry (timestamp, destinations, files, bytes,
  uploader, transfer status). `youtube_stats` summarizes totals, file kinds,
  uploaders, and recent entries, skipping malformed lines.
- **Interactive search UI** — `youtube_search_ui` exposes an embedded MCP App
  (HTML resource) for selecting videos to probe or download, with text fallback
  for hosts that cannot render the UI.
- **SSH remote transfer** — rsync with an scp fallback, non-interactive
  (`BatchMode=yes`, `StrictHostKeyChecking=accept-new`), separate audio/video
  destinations, with validated `RemoteSpec`/`RemotePath` newtypes that reject
  empty, option-like, and whitespace/control-character values.
- **Bootstrap auto-download** of yt-dlp and ffmpeg into a per-user cache
  (env override → `PATH` → cache → HTTPS download), with optional `YTDLP_SHA256`
  / `FFMPEG_SHA256` pinning of the resolved executable bytes.
- **Distribution** — GitHub releases (Linux + Windows MSVC binaries), Claude Code
  plugin, Gemini CLI extension, and an MCPB bundle (`.mcpb` / `.dxt`).
- Metadata embedding (title/artist/album/date + cover art), `Artist/Title [id]`
  organization, YouTube mix/radio URL cleaning, and a `setup` installer that
  registers the server into Claude Code, Codex, and Gemini CLI.

[Unreleased]: https://github.com/jmagar/ytdl-rmcp/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/jmagar/ytdl-rmcp/releases/tag/v0.7.0
