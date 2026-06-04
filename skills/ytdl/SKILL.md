---
name: ytdl
description: Download audio or video from YouTube, Vimeo, or any yt-dlp-supported site and rsync it to a configured SSH remote. Use when the user wants to grab/download/save/rip a song, video, album, playlist, or channel from a URL, pull audio off a YouTube link, or archive media to their server. Triggers on "download this", "grab the audio", "rip this playlist", "save this video", or any media URL the user wants pulled.
---

# yt-dlp Downloader

Download media from any [yt-dlp](https://github.com/yt-dlp/yt-dlp)-supported site as
audio, video, or both, embed proper metadata + cover art, and rsync the result to an
SSH remote configured when the plugin was enabled.

Two MCP tools are provided by the bundled `youtube-dl` server:

| Tool | Purpose |
| --- | --- |
| `youtube_download` | Download one or more URLs and rsync them to the remote. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |

## Defaults

- **Audio-first.** `mode` defaults to `audio`, codec defaults to the configured
  `audio_format` (mp3 unless changed at enable time).
- **Destinations come from plugin config.** Audio lands in the configured audio
  destination, video in the video destination. You do not normally pass `remote`,
  `dest_path`, or `video_dest_path` — they fall back to the user config.
- **Files are organized by artist.** Output is `Artist/Title [id].ext`, with title,
  artist, album, date, and cover art embedded so media servers (Plex, etc.) index
  them cleanly.

## Common usage

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

Re-pull a playlist and only fetch what's new:

```
youtube_download(urls="https://.../playlist?list=...", use_archive=true)
```

Check a target before a big download:

```
youtube_probe(urls="https://...")
```

## Notes

- **YouTube mix/radio URLs** (`list=RD...`, `&start_radio=1`) are auto-cleaned to the
  seed video so they don't resolve to an unrelated track.
- **Playlists** are downloaded fully and flattened into per-artist folders.
- On transfer failure the local staging copy is kept so the operation can be retried;
  on success it is removed unless `keep_local=true`.
- yt-dlp auto-updates at server startup when stale (configurable), so a fresh session
  self-heals against extractor breakage.

## Requirements (on the host running this plugin)

- **ffmpeg** — audio extraction, merging, metadata/cover-art embedding
- **rsync** and **ssh** (openssh-client)
- Passwordless key-based SSH auth to the configured remote
