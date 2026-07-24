---
type: "Reference"
title: "Download Workflow"
openwiki_generated: true
---

# Download Workflow

The `youtube_download` tool orchestrates the full media pipeline: resolve tools, download via yt-dlp, embed metadata, organize files, transfer to local/SSH/rclone targets, append history, and sync Plex playlists.

## Entry point

Tool calls arrive at [`mcp.rs`](../../src/mcp.rs) via the `#[tool]` macro on `YtdlServer::youtube_download`. The `DownloadInput` struct (from [`model.rs`](../../src/model.rs)) carries:

- `urls` — String or array of URLs (validated as `http`/`https`)
- `mode` — Audio, video, or both (default: audio)
- `target_path` — Audio destination (`/path`, `host:/path`, `ssh:host:/path`, or `rclone:remote:path`)
- `video_target_path` — Video destination; falls back to `target_path`
- `audio_format` — Codec (`mp3`, `m4a`, etc.; overrides `YTDLP_AUDIO_FORMAT`)
- `use_archive` — Skip already-downloaded IDs (overrides `YTDLP_USE_ARCHIVE`)

Legacy `remote`, `dest_path`, and `video_dest_path` inputs still work for
compatibility, but new integrations should use `target_path` and
`video_target_path`.

## Orchestration

[`service::run_download`](../../src/service.rs) drives the workflow:

1. **Resolve external tools** — `service::ensure_tools` calls `bootstrap::ensure` and caches yt-dlp + ffmpeg per process
2. **Prepare directories** — Create staging (tempfile) and archive (`project_dirs()`) dirs off-reactor
3. **Parse transfer target** — `TransferTarget::parse_targets` validates local, SSH, and rclone target paths
4. **Download each URL** — Loop over URLs (cleaning mix/radio params via [`urls::strip_mix_params`](../../src/urls.rs)):
   - `downloader::fetch` runs yt-dlp with `--print` JSON output
   - Metadata embedded via ffmpeg (title/artist/album/date + cover art)
   - Files organized as `Artist/Title [id].ext`
   - Optional auto-retag via AcoustID when `YTDLP_ACOUSTID_CLIENT_KEY` is set
5. **Transfer to target** — `transfer::transfer_to_target` syncs each produced audio/video subtree
6. **Record transfer queue manifest** — On transfer failure, `transfer_queue.rs` writes a server-created manifest while retained staging, manifest IDs, files, and original targets still coexist
7. **Append history** — JSONL entry written to `YTDLP_HISTORY_PATH` with file lock
8. **Sync Plex playlist** — Optional Plex add (fails soft, doesn't fail download)

## Download phase

[`downloader.rs`](../../src/downloader.rs) spawns yt-dlp subprocesses via [`util::run_capped`](../../src/util.rs) with a `--` end-of-options guard after validated URLs. yt-dlp argv includes:

- `--format` for audio/video selection and quality
- `--extract-audio` with `--audio-format` for audio codec
- `--embed-metadata` and `--embed-thumbnail` for tagging
- `--parse-metadata` for artist/title extraction
- `--print` for structured JSON output parsing
- `--download-archive` when `use_archive` is enabled
- `--windows-filenames` always on for consistent naming
- Optional `--extractor-args` from `YTDLP_EXTRACTOR_ARGS`

Output is parsed from yt-dlp's `--print` JSON into `ItemResult` structs (file paths, metadata, errors).

## Transfer phase

[`transfer.rs`](../../src/transfer.rs) handles local copies, rclone remotes, and SSH transfers:

- **Local** — Copies to an allowed local filesystem target when `YTDLP_ALLOW_LOCAL_TARGETS=true`
- **rclone** — Uses `rclone copy` for explicit `rclone:remote:path` targets
- **rsync** — Preferred for SSH when available: `rsync --protect-args -e "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new"`
- **scp** — SSH fallback for Windows or hosts without rsync: `scp -o BatchMode=yes -o StrictHostKeyChecking=accept-new`
- **Remote dirs** — Created via `ssh mkdir -p` before transfer
- **Failures** — Local staging copies are kept for retry, and a transfer queue manifest is written by the server with a redacted queue error

`youtube_transfer_queue` lists and drains those retained-staging manifests.
Retry accepts opaque manifest IDs only, re-parses the original target paths,
and re-checks `YTDLP_ALLOW_LOCAL_TARGETS` before any local transfer.

## History and stats

Every download attempt appends a JSONL entry to `YTDLP_HISTORY_PATH` (default: `~/.local/state/ytdl-mcp/downloads.jsonl`):

```json
{
  "timestamp": "2026-07-12T09:00:00Z",
  "mode": "audio",
  "target_path": "rclone:gdrive:/Music/ytdl",
  "destination": "rclone:gdrive:/Music/ytdl",
  "destinations": [
    {
      "kind": "audio",
      "target_path": "rclone:gdrive:/Music/ytdl",
      "dest_path": "rclone:gdrive:/Music/ytdl"
    }
  ],
  "transferred": true,
  "transfer_error": null,
  "partial_items": 0,
  "failed_items": 0,
  "total_files": 1,
  "total_bytes": 12345,
  "items": [
    {
      "url": "https://youtube.com/watch?v=...",
      "title": "...",
      "status": "ok",
      "files": [
        {
          "name": "Artist/Title [id].mp3",
          "kind": "audio",
          "bytes": 12345,
          "title": "Title",
          "uploader": "Artist",
          "video_id": "id",
          "duration": 210.0
        }
      ]
    }
  ]
}
```

The `youtube_stats` tool aggregates this ledger (totals, file kinds, uploaders, recent entries) via [`history::run_stats`](../../src/history.rs).

`youtube_plex_playlist list_candidates` also reads the ledger, but only projects
successful `transferred: true` audio files into stable opaque candidates. Failed
or retained-staging transfers are intentionally excluded from playlist building.

## Plex sync

When Plex credentials are configured (`YTDLP_PLEX_URL`, `YTDLP_PLEX_TOKEN`, `YTDLP_PLEX_PLAYLIST`), downloaded audio tracks are matched against the Plex library and added to the playlist (default: `yt-dlp Downloads`):

1. Search library by title + artist
2. Skip if track already in playlist
3. Add track to playlist idempotently
4. Report Plex errors without failing the download

`youtube_plex_playlist preview` uses the same resolver without mutating Plex.
`youtube_plex_playlist apply` can return token-free Plex Web and best-effort
Plexamp links. The playlist mutation path uses the official Plex Media Server
API; the `listen.plex.tv` Plexamp link shape is generated and unverified.

See [`plex.rs`](../../src/plex.rs) for the Plex client implementation.

## Error handling

- **Partial failures** — Individual URL errors are reported in the payload without failing the whole batch
- **Transfer failures** — Local staging preserved, a manifest is created for drain, and queue manifest/retry errors are redacted before queue persistence/rendering. The download payload and history preserve the original transfer error for diagnostics.
- **Plex failures** — Logged but don't fail the download
- **Bootstrap failures** — Tool resolution errors fail the download immediately (no tools, no work)
