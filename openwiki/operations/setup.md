---
type: "Reference"
title: "Setup and Configuration"
openwiki_generated: true
---

# Setup and Configuration

rytdl can be installed as a bare binary, Claude Code plugin, Gemini extension, or MCP bundle. All modes read configuration from `YTDLP_*` (and `FFMPEG_*`/`FPCALC_*`) environment variables.

## Installation modes

### Bare binary

Download a release binary from [GitHub releases](https://github.com/jmagar/rytdl/releases):

```bash
# Linux
curl -LO https://github.com/jmagar/ytdl/releases/latest/download/rytdl-linux-amd64
chmod +x rytdl-linux-amd64
sudo mv rytdl-linux-amd64 /usr/local/bin/ytdl-rmcp

# Windows
# Download rytdl-win64.exe and add to PATH
```

Run interactively:

```bash
rytdl setup  # Registers into claude/codex/gemini via mcp add
```

Run as an MCP server:

```bash
rytdl serve  # Serves MCP over stdio
```

### Claude Code plugin

Install from the `jmagar/lab` marketplace as `rytdl`. The plugin references [`scripts/run-server.sh`](../../scripts/run-server.sh), which expects `rytdl` installed in PATH.

Plugin configuration lives in [`.claude-plugin/plugin.json`](../../.claude-plugin/plugin.json) and [`.mcp.json`](../../.mcp.json) — every `user_config` key maps to an env var.

### Gemini extension

Copy [`gemini-extension.json`](../../gemini-extension.json) to the Gemini extensions directory and configure the `YTDLP_*` env vars in the extension settings.

### MCP bundle

Download the `.mcpb`/`.dxt` file from releases and install via the MCP client. The bundle targets `["linux", "win32"]` only (no macOS binary).

## Configuration

All modes read from `YTDLP_*` env vars. The canonical list is in [`config.rs`](../../src/config.rs).

### Required

- `YTDLP_REMOTE` — SSH hostname (e.g. `tootie`)
- `YTDLP_REMOTE_PATH` — Audio destination on remote (e.g. `/mnt/user/data/media/music/yt-dlp`)

### Optional

- `YTDLP_VIDEO_REMOTE_PATH` — Video destination (defaults to audio dest)
- `YTDLP_AUDIO_FORMAT` — Audio codec (`mp3`, `m4a`, etc.; default: `mp3`)
- `YTDLP_SSH_OPTS` — Extra SSH options (e.g. `-p 2222`)
- `YTDLP_STAGING_DIR` — Local staging dir (default: tempfile)
- `YTDLP_ARCHIVE_DIR` — Download archive dir (default: `~/.local/state/rytdl/archive.txt`)
- `YTDLP_HISTORY_PATH` — JSONL ledger path (default: `~/.local/state/rytdl/downloads.jsonl`)
- `YTDLP_PLEX_URL` — Plex server URL
- `YTDLP_PLEX_TOKEN` — Plex access token
- `YTDLP_PLEX_PLAYLIST` — Plex playlist name (default: `yt-dlp Downloads`)
- `YTDLP_CLEAN_METADATA` — Strip title noise like `(Official Video)` (default: `true`)
- `YTDLP_ACOUSTID_CLIENT_KEY` — AcoustID client key for auto-retag
- `FPCALC_PATH` — Path to `fpcalc` binary
- `YTDLP_MUSICBRAINZ_CONTACT` — MusicBrainz contact email
- `YTDLP_AUTO_UPDATE` — Auto-update yt-dlp/ffmpeg (default: `false`)
- `YTDLP_MAX_AGE_DAYS` — Max age for auto-update (default: `7`)
- `YTDLP_UPDATE_PRE` — Update before download (default: `true` when `auto_update` is on)
- `YTDLP_EXTRACTOR_ARGS` — Extra yt-dlp `--extractor-args` (e.g. `youtube:player_client=android`)
- `YTDLP_PATH` — Path to yt-dlp binary (overrides auto-download)
- `FFMPEG_PATH` — Path to ffmpeg binary (overrides auto-download)
- `YTDLP_TIMEOUT_SECS` — yt-dlp timeout (default: `1800`)
- `YTDLP_TRANSFER_TIMEOUT_SECS` — Transfer timeout (default: `600`)
- `YTDLP_SHA256` — Pin yt-dlp binary by SHA256
- `FFMPEG_SHA256` — Pin ffmpeg binary by SHA256

## Bootstrap

On first run, rytdl resolves yt-dlp and ffmpeg:

1. Check env override (`YTDLP_PATH`, `FFMPEG_PATH`)
2. Search `PATH` with `which`
3. Download to per-user cache (`~/.cache/rytdl/`)

SHA256 pinning is optional via `YTDLP_SHA256`/`FFMPEG_SHA256`. For strict reproducibility, combine known-good binaries with path overrides.

## Distribution packaging

Four config surfaces must stay in sync (validated by [`scripts/check-packaging.sh`](../../scripts/check-packaging.sh)):

1. **Claude plugin** — [`.claude-plugin/plugin.json`](../../.claude-plugin/plugin.json) `userConfig`
2. **MCP config** — [`.mcp.json`](../../.mcp.json) `user_config` refs and env mapping
3. **MCPB manifest** — [`mcpb/manifest.json`](../../mcpb/manifest.json) `user_config` ↔ `mcp_config.env`
4. **Gemini extension** — [`gemini-extension.json`](../../gemini-extension.json) `envVar`s

Every env var must follow the `YTDLP_`/`FFMPEG_`/`FPCALC_PATH` naming convention and map into `.mcp.json`.

## Doctor command

Run `rytdl doctor` for a read-only diagnostic report:

- Version and git SHA
- Platform
- Resolved tool paths (yt-dlp, ffmpeg, rsync, scp, ssh, fpcalc)
- Redacted config presence (which env vars are set)

Use this to triage a broken install before opening issues.
