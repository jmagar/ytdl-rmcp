# Build, Test, and Cross-Compilation

ytdl-mcp is a Rust project (edition 2021) with CI gates on fmt, clippy, and tests. Releases cross-compile Linux and Windows MSVC binaries.

## Local development

```bash
# Build
cargo build --release

# Test
cargo test

# Lint (CI gates on -D warnings)
cargo clippy --all-targets -- -D warnings

# Format check (CI gates on this)
cargo fmt --all --check
```

## Cross-compilation to Windows MSVC

Use `cargo-xwin` for Windows cross-builds from Linux:

```bash
# Install toolchain (one-time)
sudo apt-get install -y nasm llvm clang lld
cargo install cargo-xwin --locked

# Cross-build
cargo xwin build --release --target x86_64-pc-windows-msvc
```

**GOTCHA — the cargo wrapper.** `~/.local/bin/cargo` is a systemd-nspawn wrapper that breaks `cargo xwin` (manifests as `can't find crate for std`). For cross-compilation, invoke the real rustup cargo directly: `~/.cargo/bin/cargo xwin build …`.

## Windows testing

Cross-build the `.exe`, then run on a Windows VM (e.g. `agent-os`) over SSH:

```bash
ssh agent-os
# From Windows PowerShell
cd C:\Users\runner\ytdl-mcp
cargo build --release --locked
.\target\release\ytdl-mcp.exe --version
```

For stdio MCP testing, use a `Diagnostics.Process` harness that keeps stdin open and redirect stdout to a file (SSH buffers piped stdout).

## CI jobs

[`ci.yml`](../../.github/workflows/ci.yml) runs per push/PR:

- **check** — Format, clippy, and test
- **packaging** — Validate plugin/MCPB/Gemini config sync and shell scripts
- **container** — Build the Docker image
- **cross-build** — Smoke-build the Windows target (catches cross-compile breakage early)
- **windows-smoke** — Native MSVC build + run on real Windows (catches ring/rustls runtime crashes)

[`openwiki-update.yml`](../../.github/workflows/openwiki-update.yml) is a separate scheduled/manual workflow that refreshes this wiki:

- Runs `openwiki code --update --print` on a schedule
- Uses OpenRouter (`OPENWIKI_PROVIDER=openrouter`, `OPENWIKI_MODEL_ID=z-ai/glm-5.2`)
- Opens an OpenWiki docs update PR via `peter-evans/create-pull-request`

## Release process

[`release.yml`](../../.github/workflows/release.yml) runs on `v*` tags:

1. Build Linux (`cargo build --release`) and Windows MSVC (`cargo xwin build`)
2. Attach binaries to the GitHub release
3. Build the MCP bundle (`.mcpb`/`.dxt`) via `scripts/build-mcpb.sh`
4. Attach the bundle to the release

## TLS and cross-compilation gotcha

Downloads use `ureq` 3 with `rustls`+**ring** (NOT `aws-lc`). After any dep bump, verify:

```bash
cargo tree -i aws-lc-sys
```

Must be empty, or the Windows build breaks. `ffmpeg-sidecar` piggybacks on the same ureq.

## Edition 2024 migration

The project is intentionally edition 2021 for now. Do not migrate to edition 2024 unless:

- Linux tests pass
- Windows xwin build succeeds
- Plugin startup is verified

Migrating without testing all three surfaces risks breaking the distributed plugin.

## Conventions

When adding new code:

- Keep files under 500 LOC — split into `foo/` submodules if needed
- Add sibling test files (`foo_tests.rs`) wired via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`
- Use `spawn_blocking` for file I/O and subprocess runs
- Log to stderr only — stdout is the JSON-RPC channel
