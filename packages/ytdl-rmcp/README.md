# ytdl-rmcp

[![npm version](https://img.shields.io/npm/v/ytdl-rmcp.svg)](https://www.npmjs.com/package/ytdl-rmcp)
[![release](https://github.com/jmagar/ytdl-rmcp/actions/workflows/release.yml/badge.svg)](https://github.com/jmagar/ytdl-rmcp/actions/workflows/release.yml)
[![CI](https://github.com/jmagar/ytdl-rmcp/actions/workflows/ci.yml/badge.svg)](https://github.com/jmagar/ytdl-rmcp/actions/workflows/ci.yml)

A cross-platform, single-binary **MCP server** that downloads media from any
[yt-dlp](https://github.com/yt-dlp/yt-dlp)-supported site (YouTube, Vimeo, …),
embeds metadata and cover art, organizes files by artist, and transfers the
result to a local path, an SSH target, or an rclone target.

Written in Rust on the [`rmcp`](https://crates.io/crates/rmcp) crate. **yt-dlp
and ffmpeg are auto-downloaded** into a per-user cache on first run, so the host
needs neither pre-installed — the one binary is the whole install.

---

## Features

- **Audio, video, or both** — audio-first by default, with separate targets for
  audio and video.
- **Proper tagging** — embeds title / artist / album / date and cover art, and
  organizes output as `Artist/Title [id].ext` so media servers (Plex, etc.)
  index it cleanly. A non-greedy `Artist - Title` parse recovers the artist from
  free-form video titles. Source `.info.json`, thumbnail, and description
  sidecars are preserved next to the media for future retagging/indexing.
  Common YouTube title noise like `(Official Video)`, `[Official Audio]`, and
  trailing channel handles is stripped from embedded title metadata by default.
- **Self-contained paths** — the binary downloads/caches yt-dlp + ffmpeg when
  run directly; the container image bakes in ffmpeg, fpcalc, SSH, and rsync for
  media-host batch jobs.
- **Self-installing** — `ytdl-rmcp setup` registers the server into Claude Code,
  Codex, and/or Gemini CLI via each tool's own `mcp add`.
- **Robust transfers** — local paths use `rsync --partial` when present, SSH
  targets (`host:/path`) use `rsync --protect-args` with an `scp` fallback, and
  rclone targets (`remote:path` or `rclone:remote:/path`) use `rclone copy`. On transfer failure the
  local staging copy is kept for retry.
- **Repeat-safe** — `use_archive` records downloaded IDs (per mode) and skips
  them on later runs; YouTube mix/radio URLs are auto-cleaned to the seed video.
- **Stats-ready ledger** — every completed download call appends a JSONL entry
  with timestamp, destinations, files, bytes, uploader, and transfer status.
- **Plex playlist sync** — when Plex credentials are configured, downloaded
  audio is added to `yt-dlp Downloads` by default.

## Tools

| Tool | Purpose |
| --- | --- |
| `youtube_search` | Search YouTube with yt-dlp and return result URLs without downloading. |
| `youtube_search_ui` | Open an interactive YouTube search UI in MCP App-capable hosts. |
| `youtube_download` | Download one or more URLs (audio/video/both) and transfer them to a target path. |
| `youtube_probe` | Read-only: resolve title/duration/uploader/format counts without downloading. |
| `youtube_identify` | Fingerprint local audio with `fpcalc`, return AcoustID/MusicBrainz candidates, preview canonical tags, and optionally write high-confidence tags. |
| `youtube_stats` | Summarize the download ledger: totals, file kinds, uploaders, and recent entries. |
| `youtube_plex_playlist` | Build or preview Plex audio playlists from successful transferred audio history. |
| `youtube_transfer_queue` | List and drain retained-staging transfer failure manifests. |

### `youtube_download` parameters

| Param | Default | Meaning |
| --- | --- | --- |
| `urls` | — (required) | One URL string or an array of URLs. |
| `mode` | `audio` | `audio`, `video`, or `both`. |
| `audio_format` | env `YTDLP_AUDIO_FORMAT` → `mp3` | `mp3`/`m4a`/`opus`/`flac`/`wav`/`best`. |
| `audio_quality` | `0` | yt-dlp quality for lossy codecs: `0`–`9` or a bitrate like `192K`. |
| `max_height` | best | Cap video resolution (e.g. `1080`). |
| `container` | `mp4` | `mp4` or `mkv` for video. |
| `target_path` | env `YTDLP_TARGET_PATH` | Destination for audio. Use `/path` for local, `host:/path` for SSH, or `remote:path` or `rclone:remote:/path` for rclone. |
| `video_target_path` | env `YTDLP_VIDEO_TARGET_PATH` → `target_path` | Destination for video when it should land somewhere different from audio. Same target forms. |
| `keep_local` | `false` | Keep the local staging copy after transfer. |
| `use_archive` | `false` | Record + skip already-downloaded IDs (per mode). |
| `plex_playlist` | env `YTDLP_PLEX_PLAYLIST` → `yt-dlp Downloads` when Plex is configured | Plex playlist title or ID to add downloaded audio tracks to. Requires `YTDLP_PLEX_URL` and `YTDLP_PLEX_TOKEN`. |
| `response_format` | `markdown` | `markdown` or `json`. |

When Plex credentials are configured, successful downloads that produced audio
files search Plex for each downloaded track, create the target playlist if
needed, and add missing tracks while skipping entries already present. The
default playlist is `yt-dlp Downloads`; set `YTDLP_PLEX_PLAYLIST` or pass
`plex_playlist` to override it. Plex errors are reported as
`plex_playlist_error` and do not make the completed download fail. JSON
responses include a `plex_playlist` summary with `matched`, `added`,
`already_present`, and `missing` counts.

Canonical metadata matching through MusicBrainz/AcoustID is documented in
`docs/musicbrainz-acoustid.md`. `youtube_download` automatically runs
high-confidence MusicBrainz retagging for downloaded audio when
`YTDLP_ACOUSTID_CLIENT_KEY` is configured; `youtube_identify` remains available
for previewing or repairing existing library files, with manual tag writes
enabled by `write_tags=true`.

#### `youtube_download` JSON response

With `response_format=json`, the call returns a single object describing the
batch:

| Field | Meaning |
| --- | --- |
| `transferred` | `true` if every produced subtree reached its target. |
| `transfer_error` | `null` on success, else the failure/timeout message (string). |
| `target_path` / `destination` / `destinations` | The per-kind target destination(s) actually used. |
| `staging_kept_at` | Local staging path retained for retry (set when the transfer failed or `keep_local` was requested). |
| `total_files` / `total_bytes` / `total_size` | Aggregate counts across all items. |
| `partial_items` | Count of items that errored **but** still produced files. |
| `failed_items` | Count of items that errored **and** produced no files. |
| `items[]` | Per-URL results, each with a `status`, `title`, `video_id`, `error`, and a `files[]` list. |

Each `items[].status` is one of:

- `ok` — succeeded with files.
- `partial` — an error occurred but some files were still produced.
- `failed` — errored with no files.
- `skipped` — nothing new (already in the archive).

Optional keys are attached only when the relevant stage ran:

- `metadata_retag` — MusicBrainz/AcoustID auto-retag summary (`attempted`,
  `matched`, `written`, `skipped`, `errors`, or an `error` string); present when
  `YTDLP_ACOUSTID_CLIENT_KEY` is configured.
- `plex_playlist` — Plex playlist summary (`playlist`, `matched`, `added`,
  `already_present`, `missing`); `plex_playlist_error` is set instead if the
  Plex update failed (a Plex failure does not fail the download).
- `history_error` — set when the download succeeded but the JSONL ledger append
  failed.

`youtube_probe` takes `urls` and `response_format`.

### `youtube_plex_playlist`

Build or preview Plex audio playlists from successful ytdl-rmcp download
history. `list_candidates` returns audio candidates only from history entries
where `transferred` is `true`; `preview` resolves candidates against Plex
without mutation; `apply` adds matches idempotently. Failed or retained-staging
transfers are intentionally excluded.

`apply` can return `plexamp_url`, `plex_web_url`, and
`playback_link_status`. The Plex Media Server playlist calls use the official
PMS API. `plexamp_url` is a best-effort generated `listen.plex.tv` playback
link, not an official Plexamp API guarantee.

### `youtube_transfer_queue`

List and drain server-created transfer failure manifests. Actions are `list`,
`retry`, `retry_all`, and `prune`. Retry accepts only an opaque `manifest_id`,
uses the original target paths recorded at failure time, and re-checks local
target policy before transfer. `prune` removes failure manifests whose staging
directory is gone. The queue never accepts arbitrary filesystem paths.

### `youtube_identify` parameters

| Param | Default | Meaning |
| --- | --- | --- |
| `paths` | — (required) | One local audio file path string or an array of paths. |
| `write_tags` | `false` | Write high-confidence MusicBrainz tag previews back to the audio files. |
| `response_format` | `markdown` | `markdown` or `json`. |

`youtube_identify` runs Chromaprint `fpcalc`, sends the fingerprint to AcoustID,
and returns MusicBrainz recording candidates. When the best candidate is
high-confidence, it also fetches the MusicBrainz recording/release data and
includes a `retag_preview` showing the canonical artist, title, release, release
date, release type, track number, and MusicBrainz IDs. By default it is
preview-only. With `write_tags=true`, it writes the preview to the file with
Lofty, including common title/artist/album/date/track fields plus MusicBrainz
recording, release, release-group, and release-type tags. It requires
`YTDLP_ACOUSTID_CLIENT_KEY`; set `FPCALC_PATH` if `fpcalc` is not on `PATH`.

### `youtube_search` parameters

| Param | Default | Meaning |
| --- | --- | --- |
| `query` | - (required) | YouTube search text. The server passes this to yt-dlp as `ytsearchN:<query>`. |
| `limit` | `10` | Number of results, clamped to `1..=25`. |
| `response_format` | `markdown` | `markdown` or `json`. |

`youtube_search_ui` accepts the same input and returns the same search payload,
plus MCP App metadata for hosts that can render the embedded UI.

### `youtube_stats` parameters

| Param | Default | Meaning |
| --- | --- | --- |
| `limit` | `10` | Number of recent ledger entries to include, clamped to `0..=100`. |
| `response_format` | `markdown` | `markdown` or `json`. |

JSON stats include `total_downloads`, `total_files`, `total_bytes`,
`skipped_entries`, `by_kind`, `by_uploader`, and `recent`. Bucket fields include
`downloads` (compatibility alias for call count), `calls`, `items`, `files`,
`bytes`, and human-readable `size`. Malformed ledger lines are skipped and
counted instead of failing the whole stats call. If a download succeeds but the
ledger append fails, the download response still succeeds and includes
`history_error` in JSON output.

## Install

Run the guided installer through npm:

```bash
npx -y ytdl-rmcp setup
```

Or install the command globally:

```bash
npm i -g ytdl-rmcp
ytdl-rmcp setup
```

The npm package downloads the matching GitHub Release binary during
`postinstall`; the installed command is the Rust binary served through a tiny
Node launcher. You can also use the one-line installer:

```bash
curl -fsSL https://raw.githubusercontent.com/jmagar/ytdl-rmcp/main/scripts/install.sh | bash
```

Or download the binary tarball for your platform from
[Releases](https://github.com/jmagar/ytdl-rmcp/releases), or build it (see below).
The guided setup fetches yt-dlp + ffmpeg, prompts for your audio/video target
paths, detects which agent CLIs are present, and registers the server into the
ones you pick.

### Manual registration

Run without subcommands, `npx -y ytdl-rmcp` serves MCP over stdio. Register it
yourself:

```bash
# Claude Code
claude mcp add -s user ytdl-rmcp -e YTDLP_TARGET_PATH=tootie:/media/music -e YTDLP_EXTRACTOR_ARGS=youtube:player_client=android -- npx -y ytdl-rmcp
# Codex
codex  mcp add --env YTDLP_TARGET_PATH=tootie:/media/music --env YTDLP_EXTRACTOR_ARGS=youtube:player_client=android ytdl-rmcp -- npx -y ytdl-rmcp
# Gemini CLI (command is positional, env last)
gemini mcp add -s user ytdl-rmcp npx -y ytdl-rmcp -e YTDLP_TARGET_PATH=tootie:/media/music -e YTDLP_EXTRACTOR_ARGS=youtube:player_client=android
```

If you already installed a standalone binary with `npm i -g ytdl-rmcp`,
`scripts/install.sh`, or a release tarball, you can use that binary path in
place of `npx -y ytdl-rmcp`.

For raw MCP JSON configs, include the required target path env var and the
YouTube extractor override:

```json
{
  "mcpServers": {
    "ytdl-rmcp": {
      "command": "npx",
      "args": ["-y", "ytdl-rmcp"],
      "env": {
        "YTDLP_TARGET_PATH": "tootie:/mnt/user/data/media/music/yt-dlp",
        "YTDLP_VIDEO_TARGET_PATH": "tootie:/mnt/user/data/media/movies/yt-dlp",
        "YTDLP_AUTO_UPDATE": "1",
        "YTDLP_MAX_AGE_DAYS": "1",
        "YTDLP_EXTRACTOR_ARGS": "youtube:player_client=android"
      }
    }
  }
}
```

The checked-in `.mcp.json` is also a complete raw MCP profile: it declares the
same `user_config` keys used by the plugin/bundle manifests, supplies defaults
for local gateway imports, and maps every setting into the server environment.
For reliable YouTube search/probe behavior it defaults
`YTDLP_EXTRACTOR_ARGS` to `youtube:player_client=android`; official music-video
results frequently reject yt-dlp's default YouTube clients during metadata
extraction.

### Distributed forms

- **npm launcher** — `npx -y ytdl-rmcp` downloads and runs the matching
  GitHub Release binary. Run without subcommands, it serves MCP over stdio;
  `npx -y ytdl-rmcp setup` runs the guided installer. Stable releases publish
  the package from GitHub Actions with npm provenance.
- **Claude Code plugin** — `.claude-plugin/plugin.json` prompts for config via
  `userConfig`; `.mcp.json` launches `npx -y ytdl-rmcp`, which downloads the
  matching GitHub Release binary through npm.
- **Gemini CLI extension** — `gemini-extension.json`; install with
  `gemini extensions install https://github.com/jmagar/ytdl-rmcp`. MCP clients
  should prefer the npm launcher command, `npx -y ytdl-rmcp`.
- **Container image** — `ghcr.io/jmagar/ytdl-rmcp:main` is published on every
  push to `main`, or build locally with `docker build -t ytdl-rmcp:local .`. It
  includes `ffmpeg`, `fpcalc`, `openssh-client`, `rclone`, and `rsync`. See
  [`docs/container.md`](docs/container.md) for MCP and mounted-library examples.
- **MCP bundle (`.mcpb` / `.dxt`)** — `mcpb/manifest.json` defines a
  `binary`-type bundle for one-click install in MCPB-capable desktop hosts.
  Every main release publishes `ytdl-rmcp.mcpb` plus a legacy `ytdl-rmcp.dxt`
  alias; both contain the same linux + windows binaries. The bundle defaults
  optional config values to empty strings so Claude Desktop can install it
  before you fill in the destination settings. Configure at least the target
  path in the extension settings before downloading.
  Build one locally from prebuilt binaries with `scripts/build-mcpb.sh` (needs
  Node for the `@anthropic-ai/mcpb` CLI).

## Configuration (environment variables)

Only one value is required for downloads: `YTDLP_TARGET_PATH`.
`YTDLP_VIDEO_TARGET_PATH` is required only when video files should land
somewhere different from audio. `youtube_search`, `youtube_probe`, and
`youtube_identify` do not need a destination. The install manifests default
`YTDLP_EXTRACTOR_ARGS` to `youtube:player_client=android` because that client
survives common YouTube metadata blocks that make official music-video search
results look unavailable.

Target path forms:

- `/path/to/library` — local directory. Uses `rsync --partial` when available,
  otherwise a Rust filesystem copy. Requires `YTDLP_ALLOW_LOCAL_TARGETS=true` because local paths let MCP callers choose write locations.
- `host:/path/to/library` — SSH target. Uses the existing rsync/scp flow and
  honors `YTDLP_SSH_OPTS`.
- `remote:path/to/library` — rclone target with a relative remote path. Uses `rclone copy`; `rclone` must be
  configured on `PATH` for the server process.
- `rclone:remote:/path/to/library` — explicit rclone target for an absolute remote path; without the `rclone:` prefix, `host:/path` remains SSH by design.

| Var | Required? | Default | Used by | Meaning |
| --- | --- | --- | --- | --- |
| `YTDLP_TARGET_PATH` | Required for downloads | — | download | Destination for **audio**. Use `/path` for local, `host:/path` for SSH, or `remote:path` or `rclone:remote:/path` for rclone. |
| `YTDLP_VIDEO_TARGET_PATH` | Optional | falls back to `YTDLP_TARGET_PATH` | download | Destination for **video** when video files should land somewhere different from audio. Same target forms. |
| `YTDLP_REMOTE` | Deprecated | — | download | Legacy SSH remote alias or `user@host`. During migration, combines with `YTDLP_REMOTE_PATH` / `YTDLP_VIDEO_REMOTE_PATH` as explicit SSH targets. Prefer `YTDLP_TARGET_PATH`. |
| `YTDLP_REMOTE_PATH` | Deprecated | — | download | Legacy absolute SSH path for audio. Requires `YTDLP_REMOTE`; relative paths are rejected. Prefer `YTDLP_TARGET_PATH`. |
| `YTDLP_VIDEO_REMOTE_PATH` | Deprecated | — | download | Legacy absolute SSH path for video. Requires `YTDLP_REMOTE`; falls back to the audio target when unset. Prefer `YTDLP_VIDEO_TARGET_PATH`. |
| `YTDLP_ALLOW_LOCAL_TARGETS` | Optional | `0` | download | Permit local filesystem targets such as `/path`. Keep disabled unless you trust MCP callers to choose local write locations. |
| `YTDLP_AUDIO_FORMAT` | Optional | `mp3` | download | Default audio codec: `mp3`, `m4a`, `opus`, `flac`, `wav`, or `best`. |
| `YTDLP_STAGING_DIR` | Optional | system temp | download | Local directory where media is staged before transfer. On transfer failure this staging copy is kept for retry. |
| `YTDLP_SSH_OPTS` | Optional | — | download | Extra ssh options parsed with shell-word syntax and appended after forced non-interactive defaults. Example: `-i "~/.ssh/ytdl key" -o ProxyJump=media-bastion`. Malformed quoting is rejected, and command-execution footguns such as `ProxyCommand` are stripped. |
| `YTDLP_ARCHIVE_DIR` | Optional | per-user state dir | download | Directory that stores `use_archive` download archives. Separate per-mode archive files are created inside it. |
| `YTDLP_HISTORY_PATH` | Optional | per-user state dir `downloads.jsonl` | download, stats | JSONL download ledger used by `youtube_stats`. |
| `YTDLP_PLEX_URL` | Required for Plex sync | — | download | Plex server URL, e.g. `http://plex.local:32400`, used when adding audio downloads to a Plex playlist. Must be paired with `YTDLP_PLEX_TOKEN`. |
| `YTDLP_PLEX_TOKEN` | Required for Plex sync | — | download | Plex auth token for playlist/search API calls. Must be paired with `YTDLP_PLEX_URL`. |
| `YTDLP_PLEX_PLAYLIST` | Optional | `yt-dlp Downloads` when Plex URL/token are set | download | Default Plex playlist title or ID; can be overridden per call with `plex_playlist`. |
| `YTDLP_CLEAN_METADATA` | Optional | `1` | download | Strip common YouTube title noise before embedding metadata. Set to `0` to preserve source titles exactly. |
| `YTDLP_ACOUSTID_CLIENT_KEY` | Required for identify/auto-retagging | — | identify, download | AcoustID application API key. Required by `youtube_identify`; when set, `youtube_download` also writes high-confidence MusicBrainz tags to downloaded audio before transfer. |
| `FPCALC_PATH` | Optional | `fpcalc` on `PATH` | identify, download retagging | Optional explicit path to the Chromaprint `fpcalc` executable. |
| `YTDLP_MUSICBRAINZ_CONTACT` | Optional | GitHub repo URL | identify, download retagging | Contact URL/email included in MusicBrainz lookup User-Agent strings. |
| `YTDLP_AUTO_UPDATE` | Optional | `1` | tool bootstrap | Re-download yt-dlp when stale. Disable only when `YTDLP_PATH`/hash pins are managing the executable externally. |
| `YTDLP_MAX_AGE_DAYS` | Optional | `14` | tool bootstrap | Staleness threshold in days for yt-dlp auto-update. |
| `YTDLP_UPDATE_PRE` | Optional | `0` | tool bootstrap | Track yt-dlp's nightly pre-release channel instead of stable. |
| `YTDLP_EXTRACTOR_ARGS` | Required for reliable YouTube search/probe | `youtube:player_client=android` in install manifests; unset in bare process env | search, probe, download | Passed to yt-dlp `--extractor-args`. The Android YouTube client fixes common `This video is not available` metadata failures for official music videos. |
| `YTDLP_TIMEOUT_SECS` | Optional | `1800` | search, probe, download | Timeout for each yt-dlp search/probe/download command. |
| `YTDLP_TRANSFER_TIMEOUT_SECS` | Optional | `600` | download | Timeout for each transfer phase. |
| `YTDLP_PATH` | Optional | — | tool bootstrap | Use a specific yt-dlp executable instead of auto-resolution/auto-download. |
| `FFMPEG_PATH` | Optional | — | tool bootstrap, download | Use a specific ffmpeg executable instead of auto-resolution/auto-download. Probe/search do not need ffmpeg. |
| `YTDLP_SHA256` | Optional | — | tool bootstrap | Optional SHA-256 digest required for the resolved yt-dlp executable. |
| `FFMPEG_SHA256` | Optional | — | tool bootstrap | Optional SHA-256 digest required for the resolved ffmpeg executable. |
| `YTDLP_LOG` | Optional | `info` | server process | `tracing` filter written to stderr only. Never send logs to stdout because stdout is the MCP JSON-RPC channel. |

> **Maintainers:** this table is maintained **by hand**. `scripts/check-packaging.sh`
> cross-checks the machine-readable config surfaces (the Claude plugin,
> `.mcp.json` `user_config`, `gemini-extension.json`, and `mcpb/manifest.json`) and verifies this README mentions every mapped env var name.
> It does not compare README descriptions or defaults. When you add, rename, or remove a
> `YTDLP_*`, `FFMPEG_*`, `FPCALC_PATH`, or `YTDLP_LOG` env var, update this
> table manually.

### Bootstrap trust model

By default, first run resolves tools in this order: explicit env override,
`PATH`, cache, then HTTPS download from the upstream release source. Set
`YTDLP_SHA256` and/or `FFMPEG_SHA256` to require an exact executable digest
after resolution or download. These pins verify bytes on disk, but they do not
fetch upstream signatures or automatically discover trusted digests; operators
who need a fully pinned supply chain should provide known-good binaries through
`YTDLP_PATH` / `FFMPEG_PATH` plus matching SHA-256 pins, or disable yt-dlp
auto-update.

For stricter bootstrap control, combine `YTDLP_PATH` / `FFMPEG_PATH` with
matching `YTDLP_SHA256` / `FFMPEG_SHA256` pins. Hash pins verify the resolved
executable bytes; they are not upstream signature verification.

### Security posture

This server is designed to run with **trusted callers and operator-supplied
config** — it is not a hardened multi-tenant boundary.

- **Tool-call URLs reach yt-dlp.** Whatever `urls` an MCP caller passes are
  handed to yt-dlp, a powerful extraction tool. Only point callers at it that
  you trust. Tool-call URLs are validated as `http`/`https` before they reach
  yt-dlp, and every positional is passed after a `--` end-of-options separator so
  a `-`-prefixed value can't be parsed as a flag; the trust assumption above
  still holds regardless.
- **SSH is key-only and non-interactive.** SSH targets force `BatchMode=yes` and
  `StrictHostKeyChecking=accept-new`, so a TTY-less server fails fast instead of
  prompting; there is no password auth. Auth comes from your SSH key/agent and
  any options you add via `YTDLP_SSH_OPTS`.
- **Target paths are validated.** Local and SSH paths must be absolute and may
  not contain traversal or control characters. SSH remotes also reject
  option-like and whitespace/control-bearing values before anything reaches
  `ssh`/`rsync`/`scp`; remote paths are single-quote-escaped for the remote
  shell. Rclone targets must be `remote:path` or explicit `rclone:remote:/path` and reject control characters.

## Requirements

- **rsync** is recommended for local and SSH transfers; local transfers fall
  back to **cp**, and SSH transfers fall back to **scp**.
- **ssh** plus passwordless key-based auth when using `host:/path` targets.
- **rclone** plus a configured rclone remote when using `remote:path` or `rclone:remote:/path` targets.
- yt-dlp and ffmpeg are fetched automatically (override with `YTDLP_PATH` /
  `FFMPEG_PATH`, or just have them on `PATH`).
- `youtube_identify` additionally needs `fpcalc`; the container image includes
  it via `libchromaprint-tools`.

## Build from source

```bash
cargo build --release                                          # Linux/macOS
cargo test && cargo clippy --all-targets -- -D warnings        # checks

# Cross-compile to Windows from Linux (needs nasm + the LLVM toolchain):
sudo apt-get install -y nasm llvm clang lld
cargo install cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```

On dookie/local shells, `~/.local/bin/cargo` is a wrapper that can break
`cargo xwin`; use the real rustup cargo for local Windows rehearsals:

```bash
~/.cargo/bin/cargo xwin build --release --target x86_64-pc-windows-msvc
```

CI (`.github/workflows/`) runs fmt + clippy + tests and a Windows cross-build on
every push/PR, and publishes both binaries to a GitHub Release on `v*` tags.

This crate intentionally remains on Rust edition 2021 for the distributable
single-binary/plugin build. Move to edition 2024 only after proving Linux,
Windows MSVC cross-build, and plugin startup compatibility together.

## How it works

Bare invocation serves MCP over stdio; `setup` runs the installer. A
`youtube_download` call:

1. Resolves yt-dlp + ffmpeg (env override → PATH → cache → download) and
   verifies SHA-256 pins when configured.
2. Cleans mix/radio URLs, then runs yt-dlp per mode into a staging tree
   (`staging/audio`, `staging/video`) with metadata/thumbnail/archive flags,
   source metadata sidecars, and the `Artist/Title [id]` output template.
3. *(optional)* When `YTDLP_ACOUSTID_CLIENT_KEY` is set, fingerprints the
   downloaded audio and writes high-confidence MusicBrainz/AcoustID tags
   in-place — before transfer, so the target copy carries the canonical tags.
4. Transfers each kind's subtree to its target: local rsync/cp, SSH rsync/scp,
   or rclone copy.
5. *(optional)* When Plex credentials are configured and the transfer
   succeeded, adds the downloaded audio tracks to the target Plex playlist.
6. Appends the completed call to the JSONL download ledger.
7. Returns a markdown or JSON summary listing files, sizes, and the actual
   destination(s).

See `CLAUDE.md` for architecture, conventions, and gotchas.

## License

MIT — see `LICENSE`.

## Rust MCP naming pattern

This repo follows the Rust MCP server naming convention:

- Repo: `ytdl-rmcp`
- CLI alias: `rytdl`
- npm package: `ytdl-rmcp`
