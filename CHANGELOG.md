# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added a Plex playlist builder for successful transferred audio history, with
  read-only preview, idempotent apply, and best-effort Plexamp/Plex Web links.
- Added a transfer queue for retained staging directories after transfer
  failures, with list/retry/retry-all/prune actions.
- Extended the MCP app with Playlist and Transfers tabs.

## [1.0.1](https://github.com/jmagar/ytdl-rmcp/compare/v1.0.0...v1.0.1) (2026-07-11)


### Dependencies

* **deps:** bump actions/download-artifact from 4.3.0 to 8.0.1 ([#12](https://github.com/jmagar/ytdl-rmcp/issues/12)) ([3322370](https://github.com/jmagar/ytdl-rmcp/commit/3322370ce8cde64b9dd1634b40cb7d5f696b783f))
* **deps:** bump actions/upload-artifact from 4.6.2 to 7.0.1 ([#8](https://github.com/jmagar/ytdl-rmcp/issues/8)) ([9850d0d](https://github.com/jmagar/ytdl-rmcp/commit/9850d0db3bddc33ab979fbe6233b808e5f5117f4))
* **deps:** bump the cargo-dependencies group with 4 updates ([#26](https://github.com/jmagar/ytdl-rmcp/issues/26)) ([ba1a99f](https://github.com/jmagar/ytdl-rmcp/commit/ba1a99f76351bcfa06e10d201e6edcd84156f95c))
* **deps:** bump the github-actions group with 14 updates ([#27](https://github.com/jmagar/ytdl-rmcp/issues/27)) ([9e8eb0d](https://github.com/jmagar/ytdl-rmcp/commit/9e8eb0d589e3df4441c183247fb55edbb4bbcbee))

## [1.0.0](https://github.com/jmagar/ytdl-rmcp/compare/v0.7.1...v1.0.0) (2026-07-11)


### ⚠ BREAKING CHANGES

* unify transfer targets

### Added

* unify transfer targets ([4cc891e](https://github.com/jmagar/ytdl-rmcp/commit/4cc891e8e82d2621eb123dbbc6f161547b96de5f))


### Fixed

* align ytdl build workflows with rytdl binary ([f3377b5](https://github.com/jmagar/ytdl-rmcp/commit/f3377b55b1d71d3266aa8ce569274463971cbd79))
* align ytdl npm launcher assets ([e1b392c](https://github.com/jmagar/ytdl-rmcp/commit/e1b392c2e69f332e5beecf12b45d95dc62337840))
* harden target path migration ([c0af9e7](https://github.com/jmagar/ytdl-rmcp/commit/c0af9e7fcad01d04670e56139b83b740e2ed8094))

## [1.0.0] - 2026-07-11

### Security

- Validate tool-call URLs as `http`/`https` and pass every yt-dlp positional
  after a `--` end-of-options separator, so a `-`-prefixed value can't be parsed
  as a flag.
- Harden `RemotePath` against traversal for validated remote specs.

### Added

- `YTDLP_TARGET_PATH` / `YTDLP_VIDEO_TARGET_PATH` destination model, supporting
  local paths, SSH targets, and rclone targets from one setting.
- Optional `YTDLP_ALLOW_LOCAL_TARGETS` guard for per-call local filesystem
  destination overrides.
- `ytdl-rmcp doctor` subcommand: a read-only diagnostic report (version, git SHA,
  platform, resolved tool paths, and redacted config presence) for triaging a
  broken install.
- Embed the build's git SHA in `server_info`.

### Changed

- Deprecated `YTDLP_REMOTE` + `YTDLP_REMOTE_PATH` and the matching per-call
  `remote` / `dest_path` inputs in favor of target paths, while retaining
  runtime compatibility for existing installs.
- Download JSON/history now include `target_path` while retaining legacy SSH
  destination fields during the transition.
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

[Unreleased]: https://github.com/jmagar/ytdl-rmcp/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/jmagar/ytdl-rmcp/compare/v0.7.0...v1.0.0
[0.7.0]: https://github.com/jmagar/ytdl-rmcp/releases/tag/v0.7.0
