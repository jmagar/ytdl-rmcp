# youtube-dl-mcp

An MCP server that downloads media from any [yt-dlp](https://github.com/yt-dlp/yt-dlp)-supported
site (YouTube, Vimeo, etc.) as **audio, video, or both** — defaulting to audio — and then
`rsync`s the result to a directory on an SSH remote you have passwordless key-based auth for.

Built on the official MCP Python SDK (`mcp` ≥ 1.27) with FastMCP.

## Tools

| Tool | Purpose |
| --- | --- |
| `youtube_download` | Download one or more URLs (audio/video/both) and rsync them to a remote dir. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |

### `youtube_download` parameters

- `urls` — one or more URLs (a single string is accepted).
- `mode` — `audio` (default), `video`, or `both`.
- `audio_format` — `mp3` (default), `m4a`, `opus`, `flac`, `wav`, or `best` (skips re-encode).
- `audio_quality` — `0`–`9` (VBR, `0` = best) or a bitrate like `192K`. Ignored for `best`/`flac`/`wav`.
- `max_height` — cap video resolution, e.g. `1080`, `2160`. Omit for best available.
- `container` — `mp4` (default) or `mkv` for video output.
- `remote` — SSH alias (`~/.ssh/config`) or `user@host`. Falls back to `YTDLP_REMOTE`.
- `dest_path` — **absolute** remote directory. Falls back to `YTDLP_REMOTE_PATH`.
- `keep_local` — keep the staged local copy after a successful transfer (default `false`).
- `use_archive` — record downloaded IDs and skip them on future calls (default `false`).
- `response_format` — `markdown` (default) or `json`.

`mode='both'` runs two passes and produces a merged video file **and** a separately
extracted audio file per source. On transfer failure the local staging copy is kept so
you can retry; on success it's deleted unless `keep_local` is set. Per-file download
progress is reported to clients that support progress notifications.

### Download archive (`use_archive`)

When enabled, yt-dlp records each downloaded video ID and skips it next time — ideal for
re-pulling a playlist or channel and only grabbing what's new. Audio and video are tracked
in **separate** archive files (`archive-audio.txt`, `archive-video.txt`) so `both` mode
never has one pass skip the other. Archives persist in `YTDLP_ARCHIVE_DIR` (default
`~/.local/state/youtube_dl_mcp`), independent of the ephemeral staging dir.

### Auto-update

yt-dlp's YouTube extractor breaks often, so the server **updates yt-dlp at startup** when
the installed version is older than `YTDLP_MAX_AGE_DAYS` (default `90`, matching yt-dlp's
own staleness warning). This runs before yt-dlp is first imported, so a fresh launch
self-heals — which is the normal case, since MCP clients spawn the server per session.

> A `pip install -U` can't hot-swap a module already loaded in a long-running process, so
> mid-session updates only take effect on the next launch. Disable entirely with
> `YTDLP_AUTO_UPDATE=0`; set `YTDLP_UPDATE_PRE=1` to track the nightly channel. Update
> status is logged to stderr (stdout is reserved for the JSON-RPC transport).

## Requirements

- **Python** ≥ 3.11
- **ffmpeg** — audio extraction and video merging
- **rsync** and **ssh** (openssh-client) on the local host
- Passwordless key-based SSH auth to the remote (`-o BatchMode=yes` is forced, so a
  missing key fails fast instead of prompting)

```bash
# Debian/Ubuntu
sudo apt install ffmpeg rsync openssh-client
```

## Install

```bash
# from the project root
uv sync            # or: pip install -e .
```

## Configure your MCP client

`uv` (recommended — pins to this project's venv):

```json
{
  "mcpServers": {
    "youtube-dl": {
      "command": "uv",
      "args": ["--directory", "/abs/path/to/youtube-dl-mcp", "run", "youtube-dl-mcp"],
      "env": {
        "YTDLP_REMOTE": "tootie",
        "YTDLP_REMOTE_PATH": "/mnt/user/media/music",
        "YTDLP_STAGING_DIR": "/tmp"
      }
    }
  }
}
```

### Environment variables (all optional)

| Var | Default | Meaning |
| --- | --- | --- |
| `YTDLP_REMOTE` | — | Default SSH remote when `remote` isn't passed. |
| `YTDLP_REMOTE_PATH` | — | Default absolute remote destination dir. |
| `YTDLP_STAGING_DIR` | system temp | Local staging base dir. |
| `YTDLP_AUDIO_FORMAT` | `mp3` | Default audio codec. |
| `YTDLP_SSH_OPTS` | — | Extra `ssh` options, space-separated (appended after `-o BatchMode=yes`). |
| `YTDLP_ARCHIVE_DIR` | `~/.local/state/youtube_dl_mcp` | Where download archives live. |
| `YTDLP_AUTO_UPDATE` | `1` | Auto-update yt-dlp at startup when stale. |
| `YTDLP_MAX_AGE_DAYS` | `90` | Age threshold (days) that counts as stale. |
| `YTDLP_UPDATE_PRE` | `0` | Update from the nightly channel instead of stable. |

## Notes & limitations

- `dest_path` should be **absolute**; `~` is not shell-expanded (it's quoted to survive spaces).
- Playlist URLs are downloaded fully and flattened into the destination dir.
- Downloads block until complete; the work runs off the event loop so the server stays responsive.

## Test locally

```bash
# boots over stdio; Ctrl-C to exit
uv run youtube-dl-mcp

# or inspect with the MCP Inspector
npx @modelcontextprotocol/inspector uv run youtube-dl-mcp
```
