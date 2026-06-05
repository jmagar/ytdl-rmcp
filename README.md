# ytdl-mcp

A cross-platform, single-binary **MCP server** that downloads media from any
[yt-dlp](https://github.com/yt-dlp/yt-dlp)-supported site (YouTube, Vimeo, …),
embeds metadata and cover art, organizes files by artist, and transfers the
result to a directory on an SSH remote — over `rsync` (with an `scp` fallback for
hosts that lack it, e.g. Windows).

Written in Rust on the [`rmcp`](https://crates.io/crates/rmcp) crate. **yt-dlp
and ffmpeg are auto-downloaded** into a per-user cache on first run, so the host
needs neither pre-installed — the one binary is the whole install.

---

## Features

- **Audio, video, or both** — audio-first by default, with separate remote
  destinations for audio and video.
- **Proper tagging** — embeds title / artist / album / date and cover art, and
  organizes output as `Artist/Title [id].ext` so media servers (Plex, etc.)
  index it cleanly. A non-greedy `Artist - Title` parse recovers the artist from
  free-form video titles.
- **Self-contained** — downloads and caches its own yt-dlp + ffmpeg; no Python
  venv, no system packages.
- **Self-installing** — `ytdl-mcp setup` registers the server into Claude Code,
  Codex, and/or Gemini CLI via each tool's own `mcp add`.
- **Robust transfers** — `rsync --protect-args` when present, `scp` otherwise;
  non-interactive SSH (`BatchMode=yes`, `StrictHostKeyChecking=accept-new`) so a
  TTY-less server never hangs on a prompt. On transfer failure the local staging
  copy is kept for retry.
- **Repeat-safe** — `use_archive` records downloaded IDs (per mode) and skips
  them on later runs; YouTube mix/radio URLs are auto-cleaned to the seed video.

## Tools

| Tool | Purpose |
| --- | --- |
| `youtube_download` | Download one or more URLs (audio/video/both) and rsync/scp them to a remote dir. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |

### `youtube_download` parameters

| Param | Default | Meaning |
| --- | --- | --- |
| `urls` | — (required) | One URL string or an array of URLs. |
| `mode` | `audio` | `audio`, `video`, or `both`. |
| `audio_format` | env `YTDLP_AUDIO_FORMAT` → `mp3` | `mp3`/`m4a`/`opus`/`flac`/`wav`/`best`. |
| `audio_quality` | `0` | yt-dlp quality for lossy codecs: `0`–`9` or a bitrate like `192K`. |
| `max_height` | best | Cap video resolution (e.g. `1080`). |
| `container` | `mp4` | `mp4` or `mkv` for video. |
| `remote` | env `YTDLP_REMOTE` | SSH alias or `user@host`. |
| `dest_path` | env `YTDLP_REMOTE_PATH` | Absolute remote dir for audio. |
| `video_dest_path` | env `YTDLP_VIDEO_REMOTE_PATH` → `dest_path` | Absolute remote dir for video. |
| `keep_local` | `false` | Keep the local staging copy after transfer. |
| `use_archive` | `false` | Record + skip already-downloaded IDs (per mode). |
| `response_format` | `markdown` | `markdown` or `json`. |

`youtube_probe` takes `urls` and `response_format`.

## Install

Download the binary for your platform from
[Releases](https://github.com/jmagar/ytdl-mcp/releases), or build it (see below).
Then run the guided installer:

```bash
ytdl-mcp setup
```

It fetches yt-dlp + ffmpeg, prompts for your SSH remote and audio/video
destinations, detects which agent CLIs are present, and registers the server
into the ones you pick.

### Manual registration

Run bare, the binary serves MCP over stdio. Register it yourself:

```bash
# Claude Code
claude mcp add -s user ytdl-mcp -e YTDLP_REMOTE=tootie -e YTDLP_REMOTE_PATH=/media/music -- /path/to/ytdl-mcp
# Codex
codex  mcp add --env YTDLP_REMOTE=tootie --env YTDLP_REMOTE_PATH=/media/music ytdl-mcp -- /path/to/ytdl-mcp
# Gemini CLI (command is positional, env last)
gemini mcp add -s user ytdl-mcp /path/to/ytdl-mcp -e YTDLP_REMOTE=tootie -e YTDLP_REMOTE_PATH=/media/music
```

### Distributed forms

- **Claude Code plugin** — `.claude-plugin/plugin.json` prompts for config via
  `userConfig` and downloads the release binary into the plugin data dir.
- **Gemini CLI extension** — `gemini-extension.json`; install with
  `gemini extensions install https://github.com/jmagar/ytdl-mcp` (needs the
  binary on `PATH`).

## Configuration (environment variables)

| Var | Default | Meaning |
| --- | --- | --- |
| `YTDLP_REMOTE` | — | SSH remote (alias or `user@host`) for transfers. |
| `YTDLP_REMOTE_PATH` | — | Absolute remote dir for **audio**. |
| `YTDLP_VIDEO_REMOTE_PATH` | falls back to audio | Absolute remote dir for **video**. |
| `YTDLP_AUDIO_FORMAT` | `mp3` | Default audio codec. |
| `YTDLP_STAGING_DIR` | system temp | Local staging base dir. |
| `YTDLP_SSH_OPTS` | — | Extra ssh options (space-separated; appended after the forced `BatchMode`/`StrictHostKeyChecking` flags). |
| `YTDLP_ARCHIVE_DIR` | per-user state dir | Where `use_archive` history lives. |
| `YTDLP_AUTO_UPDATE` | `1` | Re-download yt-dlp when stale. |
| `YTDLP_MAX_AGE_DAYS` | `14` | Staleness threshold (days). |
| `YTDLP_UPDATE_PRE` | `0` | Track yt-dlp's nightly channel. |
| `YTDLP_EXTRACTOR_ARGS` | — | Passed to yt-dlp `--extractor-args`, e.g. `youtube:player_client=android` for videos the default clients can't reach. |
| `YTDLP_PATH` / `FFMPEG_PATH` | — | Use a specific yt-dlp / ffmpeg instead of auto-download. |
| `YTDLP_LOG` | `info` | `tracing` filter (stderr only). |

## Requirements

- **ssh** (and optionally **rsync** — falls back to **scp**, e.g. on Windows).
- Passwordless key-based SSH auth to the remote.
- yt-dlp and ffmpeg are fetched automatically (override with `YTDLP_PATH` /
  `FFMPEG_PATH`, or just have them on `PATH`).

## Build from source

```bash
cargo build --release                                          # Linux/macOS
cargo test && cargo clippy --all-targets -- -D warnings        # checks

# Cross-compile to Windows from Linux (needs nasm + the LLVM toolchain):
sudo apt-get install -y nasm llvm clang lld
cargo install cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```

CI (`.github/workflows/`) runs fmt + clippy + tests and a Windows cross-build on
every push/PR, and publishes both binaries to a GitHub Release on `v*` tags.

## How it works

Bare invocation serves MCP over stdio; `setup` runs the installer. A
`youtube_download` call:

1. Resolves yt-dlp + ffmpeg (env override → PATH → cache → download).
2. Cleans mix/radio URLs, then runs yt-dlp per mode into a staging tree
   (`staging/audio`, `staging/video`) with metadata/thumbnail/archive flags and
   the `Artist/Title [id]` output template.
3. Transfers each kind's subtree to its own remote dir (rsync, else scp).
4. Returns a markdown or JSON summary listing files, sizes, and the actual
   destination(s).

See `CLAUDE.md` for architecture, conventions, and gotchas.

## License

MIT — see `LICENSE`.
