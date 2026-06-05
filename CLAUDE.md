# ytdl-mcp — agent memory

Cross-platform single-binary MCP server: downloads media with yt-dlp, embeds
metadata + cover art, organizes by artist, and rsync/scp's to an SSH remote.
Rust, built on the `rmcp` crate. yt-dlp + ffmpeg are auto-downloaded at runtime.

User-facing docs live in `README.md`. This file is for working **on** the repo.

## Architecture (module layout)

`src/`, all files < 500 LOC, `foo.rs` + `foo/` (never `mod.rs`):

| File | Role |
| --- | --- |
| `main.rs` | clap dispatch: bare → serve stdio, `setup` → installer; stderr tracing |
| `config.rs` | `Config::from_env` — all `YTDLP_*` env vars |
| `model.rs` | tool input structs + enums (serde + schemars); `Urls` accepts string or array |
| `mcp.rs` | `rmcp` `ServerHandler` via `#[tool_router]`/`#[tool]`/`#[tool_handler]` — 2 tools |
| `service.rs` | orchestration: resolve tools → download → transfer → format payload |
| `downloader.rs` | builds the yt-dlp argv, runs it, parses `--print` output; `fetch` + `probe` |
| `transfer.rs` | rsync-or-scp, `ensure_remote_dir` |
| `bootstrap.rs` + `bootstrap/{ytdlp,ffmpeg,http}.rs` | resolve/install yt-dlp + ffmpeg into the cache dir |
| `urls.rs` | YouTube mix/radio URL cleaning |
| `setup.rs` | interactive installer; registers into claude/codex/gemini via `mcp add` |
| `util.rs` | shared `command_error` |

Tests are sibling `foo_tests.rs` files wired via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`.

## Conventions

- **No file over 500 LOC.** Split into a `foo/` dir with submodules instead.
- **No `mod.rs`** — `foo.rs` declares `mod bar;` resolving to `foo/bar.rs`.
- **Sibling test files** — `foo_tests.rs` next to `foo.rs`, never inline `mod tests {}`.
- **stdout is the JSON-RPC channel** — ALL logging goes to **stderr**
  (`tracing_subscriber ... .with_writer(std::io::stderr)`). Never print to stdout
  outside the MCP transport, and never forward yt-dlp's captured stdout.

## Build / test / cross-compile

```bash
cargo build --release
cargo test                                    # 15 tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check                       # CI gates on this

# Windows cross-build (needs: apt install nasm llvm clang lld; cargo install cargo-xwin):
cargo xwin build --release --target x86_64-pc-windows-msvc
```

**GOTCHA — the cargo wrapper.** `~/.local/bin/cargo` is a wrapper that runs
builds inside a constrained systemd slice and breaks `cargo xwin` (manifests as
`error[E0463]: can't find crate for std` on one dep). For cross-compilation,
invoke the real rustup cargo directly: `~/.cargo/bin/cargo xwin build …`.

## Key gotchas

- **TLS / cross-compile**: downloads use `ureq` 3 with `rustls`+**ring** (NOT
  aws-lc). ffmpeg-sidecar piggybacks on the same ureq. Verify after any dep bump:
  `cargo tree -i aws-lc-sys` must be empty, or the Windows build breaks.
- **`--windows-filenames` is always on** so the `Artist/Title [id]` layout is
  identical across OSes. Side effect: a trailing `.` in a name (e.g. "Disney Jr.")
  becomes "Disney Jr.#".
- **Some videos need a specific yt-dlp player client** (e.g. Disney content fails
  on the default but works with `youtube:player_client=android`). Surface via the
  `YTDLP_EXTRACTOR_ARGS` env var (`--extractor-args`).
- **Probe doesn't download ffmpeg** — `youtube_probe` calls `bootstrap::ensure_ytdlp`
  (yt-dlp only); only `youtube_download` pulls ffmpeg.
- **Testing the stdio server**: a piped-stdin smoke test EOFs and rmcp closes
  after a ~5s drain — slow first-run downloads get cut off. Hold stdin open
  (`{ printf …; sleep N; } | bin serve`) or use `mcporter` (real MCP client).
- **Windows testing**: cross-build the `.exe`, run it on **agent-os** (the Windows
  VM) over `ssh agent-os` — serve via a `Diagnostics.Process` harness that keeps
  stdin open and redirect stdout to a file (SSH buffers piped stdout).

## Distribution

- **GitHub**: `jmagar/ytdl-mcp`. Release CI in `.github/workflows/release.yml`
  builds linux + windows-msvc and attaches to `v*` releases; `ci.yml` runs
  fmt/clippy/test + a Windows cross-build smoke per push/PR.
- **Claude Code plugin**: root `.claude-plugin/`, `.mcp.json`, `hooks/`,
  `scripts/` (`fetch-binary.sh` downloads the release binary into
  `CLAUDE_PLUGIN_DATA`; `run-server.sh` execs it). Registered in the
  `jmagar/lab` marketplace as `ytdl-mcp`.
- **Gemini extension**: `gemini-extension.json` (settings → `YTDLP_*` env vars).

## Per-CLI `mcp add` arg ordering (setup.rs)

Each CLI parses repeated/variadic env flags differently:
- claude: `mcp add -s user <name> -e K=V… -- <cmd>`
- codex:  `mcp add --env K=V… <name> -- <cmd>`
- gemini: `mcp add -s user <name> <cmd> -e K=V…` (env array goes last)
