---
type: "Reference"
title: "Container Runtime"
openwiki_generated: true
---

# Container Runtime

The container image packages `ytdl-mcp` with ffmpeg, fpcalc, SSH, rsync, and CA certificates for production media-host batch jobs. The server still runs MCP over stdio by default.

## Published image

Every push to `main` publishes to the GitHub Container Registry:

```bash
ghcr.io/jmagar/rytdl:main
ghcr.io/jmagar/rytdl:main-<git-sha>
```

Pull the latest:

```bash
docker pull ghcr.io/jmagar/rytdl:main
```

## Build locally

```bash
docker build -t ytdl-rmcp:local .
```

The base image is pinned by digest in [`Dockerfile`](../../Dockerfile) for supply-chain security.

## Run as an MCP server

Mount SSH credentials for remote transfers and keep state/cache directories so yt-dlp, ffmpeg, the ledger, and archives survive container restarts:

```bash
docker run --rm -i \
  -e YTDLP_REMOTE=tootie \
  -e YTDLP_REMOTE_PATH=/mnt/user/data/media/music/yt-dlp \
  -e YTDLP_HISTORY_PATH=/home/ytdl/.local/state/ytdl/downloads.jsonl \
  -v "$HOME/.ssh:/home/ytdl/.ssh:ro" \
  -v ytdl-state:/home/ytdl/.local/state/ytdl \
  -v ytdl-cache:/home/ytdl/.cache \
  ghcr.io/jmagar/rytdl:main serve
```

For MCP clients that expect a command, use `docker run --rm -i ... ghcr.io/jmagar/rytdl:main serve`.

## Volume mounts

- **SSH credentials** — `~/.ssh:/home/ytdl/.ssh:ro` (read-only)
- **State dir** — Named volume for the JSONL ledger and archive file
- **Cache dir** — Named volume for yt-dlp/ffmpeg sidecars and AcoustID/MusicBrainz caches

## Identify a mounted library

`youtube_identify` reads local paths from inside the container. Mount the library and pass container paths:

```bash
docker run --rm -i \
  -e YTDLP_ACOUSTID_CLIENT_KEY="$YTDLP_ACOUSTID_CLIENT_KEY" \
  -e YTDLP_MUSICBRAINZ_CONTACT="you@example.com" \
  -v /mnt/user/data/media/music/yt-dlp:/library \
  ghcr.io/jmagar/rytdl:main serve
```

Then call:

```json
{
  "paths": "/library/Artist/Song [id].mp3",
  "write_tags": false,
  "response_format": "json"
}
```

Run with `write_tags=false` first and save the response as a report. After reviewing candidates, rerun with `write_tags=true` for accepted files.

## Batch shape for the existing yt-dlp library

The current yt-dlp audio library on `tootie` is at `/mnt/user/data/media/music/yt-dlp`. A safe batch pass:

1. Inventory audio files under the mounted library
2. Call `youtube_identify` with `write_tags=false`
3. Save one JSONL row per file with candidates, preview, and errors
4. Write tags only for high-confidence reviewed rows

This avoids blindly mutating files where AcoustID returns multiple plausible releases or soundtrack variants.

## Tools baked into the image

The container includes:

- **ffmpeg** — For metadata embedding and thumbnail attachment
- **fpcalc** (from `libchromaprint-tools`) — For AcoustID fingerprinting
- **openssh-client** — For rsync/scp transfers
- **rsync** — For efficient transfers (with scp fallback)
- **CA certificates** — For HTTPS bootstrap downloads

yt-dlp is still auto-downloaded to the cache dir on first run (env override → PATH → cache → download).

## Security notes

- SSH credentials are mounted read-only
- Non-interactive SSH (`BatchMode=yes`, `StrictHostKeyChecking=accept-new`) prevents hanging
- Base image is pinned by digest
- Container runs as non-root `ytdl` user
