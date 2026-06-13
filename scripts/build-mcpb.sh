#!/usr/bin/env bash
# Assemble the ytdl-mcp MCP bundle (.mcpb) from prebuilt release binaries.
#
# A .mcpb is a ZIP of mcpb/manifest.json plus a server/ dir holding the
# per-platform binaries. The manifest declares server.type "binary" and selects
# server/ytdl-mcp on linux and server/ytdl-mcp.exe on win32 via platform_overrides,
# so both binaries must be staged for the bundle to work on both platforms.
#
# Inputs (override via env):
#   LINUX_BIN    path to the linux x86_64 binary   (default target/release/ytdl-mcp)
#   WINDOWS_BIN  path to the windows x86_64 .exe    (default target/x86_64-pc-windows-msvc/release/ytdl-mcp.exe)
#   OUT          output bundle path                 (default ytdl-mcp.mcpb)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() { echo "[build-mcpb] $*" >&2; }
fail() {
  echo "[build-mcpb] error: $*" >&2
  exit 1
}

LINUX_BIN="${LINUX_BIN:-target/release/ytdl-mcp}"
WINDOWS_BIN="${WINDOWS_BIN:-target/x86_64-pc-windows-msvc/release/ytdl-mcp.exe}"
OUT="${OUT:-ytdl-mcp.mcpb}"
MANIFEST="mcpb/manifest.json"

[ -f "$MANIFEST" ] || fail "missing $MANIFEST"
[ -f "$LINUX_BIN" ] || fail "linux binary not found: $LINUX_BIN (set LINUX_BIN)"
[ -f "$WINDOWS_BIN" ] || fail "windows binary not found: $WINDOWS_BIN (set WINDOWS_BIN)"

build_dir="$(mktemp -d)"
trap 'rm -rf "$build_dir"' EXIT

mkdir -p "$build_dir/server"
cp "$MANIFEST" "$build_dir/manifest.json"
cp "$LINUX_BIN" "$build_dir/server/ytdl-mcp"
cp "$WINDOWS_BIN" "$build_dir/server/ytdl-mcp.exe"
chmod +x "$build_dir/server/ytdl-mcp"
log "staged manifest + linux/windows binaries"

# The mcpb CLI validates the manifest against the official schema, then zips.
npx -y @anthropic-ai/mcpb validate "$build_dir/manifest.json"
npx -y @anthropic-ai/mcpb pack "$build_dir" "$OUT"

sha256sum "$OUT" > "$OUT.sha256"
log "wrote $OUT and $OUT.sha256"
