# Download Workflow

The `youtube_download` tool orchestrates the full media pipeline: resolve tools, download via yt-dlp, embed metadata, organize files, transfer to remotes, append history, and sync Plex playlists.

## Entry point

Tool calls arrive at [`mcp.rs`](../../src/mcp.rs) via the `#[tool]` macro on `YtdlServer::youtube_download`. The `DownloadInput` struct (from [`model.rs`](../../src/model.rs)) carries:

- `urls` — String or array of URLs (validated as `http`/`https`)
- `mode` — Audio, video, or both (default: audio)
- `remote` — SSH hostname (overrides `YTDLP_REMOTE`)
- `dest_path` — Audio destination on remote (overrides `YTDLP_REMOTE_PATH`)
- `video_dest_path` — Video destination (overrides `YTDLP_VIDEO_REMOTE_PATH`)
- `audio_format` — Codec (`mp3`, `m4a`, etc.; overrides `YTDLP_AUDIO_FORMAT`)
- `use_archive` — Skip already-downloaded IDs (overrides `YTDLP_USE_ARCHIVE`)

## Orchestration

[`service::run_download`](../../src/service.rs) drives the workflow:

1. **Resolve external tools** — `bootstrap::ensure_tools` caches yt-dlp + ffmpeg per process
2. **Prepare directories** — Create staging (tempfile) and archive (`project_dirs()`) dirs off-reactor
3. **Parse transfer target** — `TransferTarget::parse` validates remote spec and destination paths
4. **Download each URL** — Loop over URLs (cleaning mix/radio params via [`urls::strip_mix_params`](../../src/urls.rs)):
   - `downloader::fetch` runs yt-dlp with `--print` JSON output
   - Metadata embedded via ffmpeg (title/artist/album/date + cover art)
   - Files organized as `Artist/Title [id].ext`
   - Optional auto-retag via AcoustID when `YTDLP_ACOUSTID_CLIENT_KEY` is set
5. **Transfer to remote** — `transfer::transfer_items` rsync's/scp's to audio/video destinations
6. **Append history** — JSONL entry written to `YTDLP_HISTORY_PATH` with file lock
7. **Sync Plex playlist** — Optional Plex add (fails soft, doesn't fail download)

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

[`transfer.rs`](../../src/transfer.rs) runs rsync or scp per platform:

- **rsync** — Preferred when available: `rsync --protect-args -e "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new"`
- **scp** — Fallback for Windows or hosts without rsync: `scp -o BatchMode=yes -o StrictHostKeyChecking=accept-new`
- **Remote dirs** — Created via `ssh mkdir -p` before transfer
- **Failures** — Local staging copies kept for retry; errors reported without failing the whole batch

## History and stats

Every successful download appends a JSONL entry to `YTDLP_HISTORY_PATH` (default: `~/.local/state/ytdl-mcp/downloads.jsonl`):

```json
{
  "timestamp": "2025-01-07T09:00:00Z",
  "mode": "audio",
  "remote": "tootie",
  "audio_dest": "/mnt/user/data/media/music/yt-dlp",
  "video_dest": null,
  "items": [
    {
      "url": "https://youtube.com/watch?v=...",
      "id": "...",
      "title": "...",
      "uploader": "...",
      "files": ["Artist/Title [id].mp3"],
      "bytes": 12345,
      "error": null
    }
  ],
  "transfer_status": "success"
}
```

The `youtube_stats` tool aggregates this ledger (totals, file kinds, uploaders, recent entries) via [`history::run_stats`](../../src/history.rs).

## Plex sync

When Plex credentials are configured (`YTDLP_PLEX_URL`, `YTDLP_PLEX_TOKEN`, `YTDLP_PLEX_PLAYLIST`), downloaded audio tracks are matched against the Plex library and added to the playlist (default: `yt-dlp Downloads`):

1. Search library by title + artist
2. Skip if track already in playlist
3. Add track to playlist
4. Report Plex errors without failing the download

See [`plex.rs`](../../src/plex.md) for the Plex client implementation.

## Error handling

- **Partial failures** — Individual URL errors are reported in the payload without failing the whole batch
- **Transfer failures** — Local staging preserved; error reported with file list
- **Plex failures** — Logged but don't fail the download
- **Bootstrap failures** — Tool resolution errors fail the download immediately (no tools, no work)
