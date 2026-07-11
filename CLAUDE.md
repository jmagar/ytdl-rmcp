# ytdl-rmcp — agent memory

Cross-platform single-binary MCP server: downloads media with yt-dlp, embeds
metadata + cover art, organizes by artist, and rsync/scp's to an SSH remote.
Rust, built on the `rmcp` crate. yt-dlp + ffmpeg are auto-downloaded at runtime.

User-facing docs live in `README.md`. This file is for working **on** the repo.

## Long-Lived Branches

- `marketplace-no-mcp` is an intentional long-lived marketplace variant branch,
  not stale cleanup. It keeps the ytdl-rmcp plugin/skill surface available while
  removing bundled MCP server registration for environments where the server is
  already connected through the Labby gateway.
- Do not merge `marketplace-no-mcp` into `main` by default, and do not delete it
  as stale unless Jacob explicitly retires the no-MCP marketplace variant.

## Architecture (module layout)

`src/`, all files < 500 LOC, `foo.rs` + `foo/` (never `mod.rs`):

| File | Role |
| --- | --- |
| `main.rs` | clap dispatch: bare → serve stdio, `setup` → installer, `doctor` → diagnostics; stderr tracing |
| `config.rs` | `Config::from_env_result` — all `YTDLP_*` env vars (the panicking `from_env` is now `#[cfg(test)]`-only) |
| `doctor.rs` | `ytdl-rmcp doctor` — read-only install/diagnostics probe: prints version/git-sha, platform, resolved tool paths, and redacted config presence |
| `model.rs` | tool input structs + enums (serde + schemars); `Urls` accepts string or array |
| `mcp.rs` | `rmcp` `ServerHandler` via `#[tool_router]`/`#[tool]`/`#[tool_handler]` — 6 tools (`youtube_download`, `youtube_probe`, `youtube_identify`, `youtube_search`, `youtube_stats`, `youtube_search_ui`) |
| `service.rs` | orchestration: resolve tools → download → transfer → format payload |
| `service/format.rs` | render the response payload as JSON or Markdown per `ResponseFormat` |
| `downloader.rs` | builds the yt-dlp argv, runs it, parses `--print` output; `fetch` (download) path |
| `downloader/probe.rs` | `ProbeResult` + `probe`: metadata-only yt-dlp query (no media download) |
| `transfer.rs` | rsync-or-scp, `ensure_remote_dir` |
| `history.rs` | persistent JSONL download ledger + `youtube_stats` aggregation derived from it |
| `identify.rs` | AcoustID fingerprint (fpcalc) → MusicBrainz lookup → retag preview; backs `youtube_identify` |
| `identify/musicbrainz.rs` | MusicBrainz REST client + `RetagPreview` scoring |
| `identify/tagger.rs` | writes retag-preview tags into the audio file via `lofty` |
| `plex.rs` | optional Plex playlist integration — match + add downloaded tracks |
| `search_app.rs` | MCP-app HTML resource (`ui://…/youtube-search.html`) backing `youtube_search_ui` |
| `bootstrap.rs` + `bootstrap/{ytdlp,ffmpeg,http}.rs` | resolve/install yt-dlp + ffmpeg into the cache dir |
| `urls.rs` | YouTube mix/radio URL cleaning |
| `setup.rs` | interactive installer; registers into claude/codex/gemini via `mcp add` |
| `util.rs` | shared `command_error` + the single subprocess runner (`run_capped`) used by the downloader, probe, fingerprinter, and transfer paths |

Tests are sibling `foo_tests.rs` files wired via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`.

## Conventions

- **No file over 500 LOC.** Split into a `foo/` dir with submodules instead.
- **No `mod.rs`** — `foo.rs` declares `mod bar;` resolving to `foo/bar.rs`.
- **Sibling test files** — `foo_tests.rs` next to `foo.rs`, never inline `mod tests {}`.
  A large module MAY also carry extra focused test files under its `foo/` submodule
  dir (e.g. `service/render_tests.rs`, `service/stats_identify_tests.rs`), each wired
  with its own `#[cfg(test)] #[path = "service/render_tests.rs"] mod render_tests;`,
  in addition to the canonical sibling `service_tests.rs`.
- **stdout is the JSON-RPC channel** — ALL logging goes to **stderr**
  (`tracing_subscriber ... .with_writer(std::io::stderr)`). Never print to stdout
  outside the MCP transport, and never forward yt-dlp's captured stdout.

## Build / test / cross-compile

```bash
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check                       # CI gates on this

# Windows cross-build (needs: apt install nasm llvm clang lld; cargo install cargo-xwin):
cargo xwin build --release --target x86_64-pc-windows-msvc
```

The plain `cargo xwin` form above is correct for CI and ordinary shells.
**GOTCHA — the cargo wrapper.** `~/.local/bin/cargo` is a wrapper that runs
builds inside a constrained systemd slice and breaks `cargo xwin` (manifests as
`error[E0463]: can't find crate for std` on one dep). For cross-compilation,
invoke the real rustup cargo directly: `~/.cargo/bin/cargo xwin build …`.

## Key gotchas

- **TLS / cross-compile**: downloads use `ureq` 3 with `rustls`+**ring** (NOT
  aws-lc). ffmpeg-sidecar piggybacks on the same ureq. Verify after any dep bump:
  `cargo tree -i aws-lc-sys` must be empty, or the Windows build breaks.
- **Bootstrap trust**: `YTDLP_SHA256` and `FFMPEG_SHA256` optionally pin the
  resolved executable bytes. This is hash pinning, not upstream signature
  verification; known-good binaries plus `YTDLP_PATH` / `FFMPEG_PATH` are the
  strictest supported mode.
- **Timeouts**: `YTDLP_TIMEOUT_SECS` defaults to 1800 and is enforced for
  yt-dlp download/probe commands. `YTDLP_TRANSFER_TIMEOUT_SECS` defaults to 600
  and is enforced around each transfer phase from `service.rs`.
- **Rust edition 2021 is intentional for now**: this is a distributable
  single-binary MCP/plugin that is cross-built for Linux and Windows MSVC. Do not
  migrate to edition 2024 unless Linux checks, Windows xwin build, and plugin
  startup are all verified together.
- **`--windows-filenames` is always on** so the `Artist/Title [id]` layout is
  identical across OSes. Side effect: a trailing `.` in a name (e.g. "Disney Jr.")
  becomes "Disney Jr.#".
- **Some videos need a specific yt-dlp player client** (e.g. Disney content fails
  on the default but works with `youtube:player_client=android`). Surface via the
  `YTDLP_EXTRACTOR_ARGS` env var (`--extractor-args`).
- **Probe doesn't download ffmpeg** — the probe path (`src/downloader/probe.rs`,
  driven from `service.rs`) resolves only `bootstrap::ensure_ytdlp` (yt-dlp); only
  `youtube_download` pulls ffmpeg.
- **Testing the stdio server**: a piped-stdin smoke test EOFs and rmcp closes
  after a ~5s drain — slow first-run downloads get cut off. Hold stdin open
  (`{ printf …; sleep N; } | bin serve`) or use `mcporter` (real MCP client).
- **Windows testing**: cross-build the `.exe`, run it on **agent-os** (the Windows
  VM) over `ssh agent-os` — serve via a `Diagnostics.Process` harness that keeps
  stdin open and redirect stdout to a file (SSH buffers piped stdout).

## Distribution

- **GitHub**: `jmagar/ytdl-rmcp`. Release CI in `.github/workflows/release.yml`
  builds linux + windows-msvc and attaches to `v*` releases; `ci.yml` runs
  fmt/clippy/test + a Windows cross-build smoke per push/PR.
- **npm launcher**: `packages/ytdl-rmcp` publishes `ytdl-rmcp` to npm. MCP clients
  should launch with `npx -y ytdl-rmcp`; the npm postinstall/lazy installer
  downloads the matching GitHub Release binary.
- **Claude Code plugin**: root `.claude-plugin/`, `.mcp.json`, `hooks/`;
  `.mcp.json` uses `npx -y ytdl-rmcp` plus plugin `userConfig` env mapping.
  Registered in the `jmagar/lab` marketplace as `ytdl-rmcp`.
- **Gemini extension**: `gemini-extension.json` (settings → `YTDLP_*` env vars);
  prefer the npm launcher command for MCP stdio registration.
- **MCP bundle**: `mcpb/manifest.json` (`server.type: "binary"`, manifest schema
  `0.3`). `scripts/build-mcpb.sh` stages the linux + windows binaries into
  `server/` and runs the `@anthropic-ai/mcpb` CLI to produce `ytdl-rmcp.mcpb`; the
  `mcpb` job in `release.yml` attaches it to `v*` releases. Targets
  `["linux", "win32"]` only — no macOS binary is built. `check-packaging.sh`
  cross-checks all four config surfaces: the Claude plugin's `userConfig`
  (`.claude-plugin/plugin.json`), the `.mcp.json` `user_config` references and env
  mapping, the mcpb manifest's `user_config` keys ↔ `mcp_config.env`, and
  `gemini-extension.json`'s `envVar`s — verifying they stay in sync and that every
  Gemini env var follows the `YTDLP_`/`FFMPEG_`/`FPCALC_PATH` naming and maps into
  `.mcp.json`.

## Per-CLI `mcp add` arg ordering (setup.rs)

Each CLI parses repeated/variadic env flags differently:
- claude: `mcp add -s user <name> -e K=V… -- <cmd>`
- codex:  `mcp add --env K=V… <name> -- <cmd>`
- gemini: `mcp add -s user <name> <cmd> -e K=V…` (env array goes last)
