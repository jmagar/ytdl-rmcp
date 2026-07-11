---
name: ytdl
description: Download audio or video from YouTube, Vimeo, or any yt-dlp-supported site and transfer it to a configured local, SSH, or rclone target. Use when the user wants to grab/download/save/rip a song, video, album, playlist, or channel from a URL, pull audio off a YouTube link, or archive media to their server. Triggers on "download this", "grab the audio", "rip this playlist", "save this video", or any media URL the user wants pulled.
---

# yt-dlp Downloader

Download media from any [yt-dlp](https://github.com/yt-dlp/yt-dlp)-supported site as
audio, video, or both, embed proper metadata + cover art, and transfer the result
to a target path configured when the plugin was enabled.

Six MCP tools are provided by the bundled `ytdl-rmcp` server:

| Tool | Purpose |
| --- | --- |
| `youtube_search` | Search YouTube and return result URLs without downloading. |
| `youtube_search_ui` | Open an interactive YouTube search UI in MCP App-capable hosts. |
| `youtube_download` | Download one or more URLs and transfer them to the configured target. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |
| `youtube_identify` | Fingerprint local audio with `fpcalc`, return AcoustID/MusicBrainz candidates, preview canonical tags, and optionally write high-confidence tags. |
| `youtube_stats` | Summarize the persistent download ledger with totals, kinds, uploaders, and recent entries. |

## Defaults

- **Audio-first.** `mode` defaults to `audio`, codec defaults to the configured
  `audio_format` (mp3 unless changed at enable time).
- **Destinations come from plugin config.** Audio lands in `target_path`, and
  video lands in `video_target_path` when configured. Targets can be local
  (`/path`), SSH (`host:/path`), or rclone (`remote:path` or `rclone:remote:/path`). You do not normally
  pass target fields — they fall back to the user config.
- **Files are organized by artist.** Output is `Artist/Title [id].ext`, with title,
  artist, album, date, and cover art embedded so media servers (Plex, etc.) index
  them cleanly.

## Common usage

Search YouTube first:

```text
youtube_search(query="slow pulp live", limit=5)
```

Open the interactive search UI:

```text
youtube_search_ui(query="slow pulp live", limit=10)
```

Download audio (the default) from a link:

```
youtube_download(urls="https://www.youtube.com/watch?v=...")
```

Download video at capped resolution:

```
youtube_download(urls="https://...", mode="video", max_height=1080)
```

Grab both audio and video (audio → music dest, video → movies dest):

```
youtube_download(urls="https://...", mode="both")
```

Add downloaded audio tracks to a Plex playlist:

```
youtube_download(urls="https://...", plex_playlist="Fresh Downloads")
```

Re-pull a playlist and only fetch what's new:

```
youtube_download(urls="https://.../playlist?list=...", use_archive=true)
```

Check a target before a big download:

```
youtube_probe(urls="https://...")
```

Identify a local audio file against AcoustID/MusicBrainz:

```
youtube_identify(paths="/path/to/song.mp3", response_format="json")
```

High-confidence matches include a read-only `retag_preview` with canonical
MusicBrainz artist/title/release/date/type/track metadata and MBIDs.
Set `write_tags=true` to write that high-confidence preview back to the file.

Review download totals and recent entries:

```
youtube_stats(limit=10)
```

When requesting JSON stats, expect top-level totals plus `skipped_entries`,
`by_kind`, `by_uploader`, and `recent`. Bucket fields include `downloads`
(compatibility alias for call count), `calls`, `items`, `files`, `bytes`, and
`size`. Malformed ledger lines are skipped; successful downloads still return if
the ledger append fails, with `history_error` included in JSON output.

## Notes

- **YouTube mix/radio URLs** (`list=RD...`, `&start_radio=1`) are auto-cleaned to the
  seed video so they don't resolve to an unrelated track.
- **Playlists** are downloaded fully and flattened into per-artist folders.
- On transfer failure the local staging copy is kept so the operation can be retried;
  on success it is removed unless `keep_local=true`.
- Completed download calls are appended to a JSONL ledger, defaulting to the
  per-user state dir. Set `YTDLP_HISTORY_PATH` to put it somewhere specific.
- Embedded title metadata strips common YouTube noise such as `(Official Video)`,
  `[Official Audio]`, trailing `| @channel`, and extra whitespace by default.
  Set `YTDLP_CLEAN_METADATA=0` to preserve source titles exactly.
- `youtube_identify` requires `YTDLP_ACOUSTID_CLIENT_KEY` and `fpcalc`
  (Chromaprint) on `PATH`, or an explicit `FPCALC_PATH`. It previews by default;
  pass `write_tags=true` to write high-confidence MusicBrainz tags to files.
- Set `YTDLP_PLEX_URL` and `YTDLP_PLEX_TOKEN` to add downloaded audio tracks to
  the `yt-dlp Downloads` Plex playlist by default. Set `YTDLP_PLEX_PLAYLIST` or
  pass per-call `plex_playlist` to override it. Plex playlist failures are
  reported without failing a completed download.
- yt-dlp auto-updates at server startup when stale (configurable), so a fresh session
  self-heals against extractor breakage.
- yt-dlp and ffmpeg are resolved automatically: explicit env path, then `PATH`,
  then the per-user cache, then runtime download. Use `YTDLP_PATH` and
  `FFMPEG_PATH` only when you need known local binaries.

## Operational controls

- `YTDLP_TIMEOUT_SECS` controls each yt-dlp probe/download command timeout
  (default: 1800).
- `YTDLP_TRANSFER_TIMEOUT_SECS` controls each SSH transfer phase timeout
  (default: 600).
- `YTDLP_PATH` and `FFMPEG_PATH` override auto-resolution/auto-download with
  specific local binaries.
- `YTDLP_SHA256` and `FFMPEG_SHA256` optionally require exact SHA-256 digests for
  the resolved yt-dlp and ffmpeg executables.
- `YTDLP_EXTRACTOR_ARGS` is passed to yt-dlp `--extractor-args`, for example
  `youtube:player_client=android` when the default YouTube clients cannot fetch
  a video.
- `YTDLP_SSH_OPTS` adds extra SSH options using shell-word syntax, for example
  `-i "~/.ssh/ytdl key" -o ProxyJump=media-bastion`. Malformed quoting is
  rejected.
- `YTDLP_PLEX_URL`, `YTDLP_PLEX_TOKEN`, and `YTDLP_PLEX_PLAYLIST` control
  optional Plex playlist updates after successful audio transfers.

## Requirements (on the host running this plugin)

- For `host:/path` targets: **ssh** / openssh-client and passwordless key-based
  SSH auth to the configured remote.
- For `remote:path` or `rclone:remote:/path` targets: **rclone** on `PATH` with the named remote
  configured.
- **rsync** is optional; SSH transfers fall back to **scp** when rsync is
  unavailable, and local transfers fall back to Rust filesystem copy.
- yt-dlp and ffmpeg are auto-resolved/auto-downloaded unless overridden with
  `YTDLP_PATH` / `FFMPEG_PATH`
