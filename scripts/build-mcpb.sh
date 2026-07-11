#!/usr/bin/env bash
# Assemble the ytdl-rmcp MCP bundle (.mcpb) from prebuilt release binaries.
#
# A .mcpb is a ZIP of mcpb/manifest.json plus a server/ dir holding the
# per-platform binaries. The manifest declares server.type "binary" and selects
# server/rytdl on linux and server/rytdl.exe on win32 via platform_overrides,
# so both binaries must be staged for the bundle to work on both platforms.
#
# Claude Desktop for Windows currently no-ops on local .mcpb files with large
# deflated entries, so we validate with the MCPB CLI but create the archive with
# stored entries.
#
# Inputs (override via env):
#   LINUX_BIN    path to the linux x86_64 binary   (default target/release/rytdl)
#   WINDOWS_BIN  path to the windows x86_64 .exe    (default target/x86_64-pc-windows-msvc/release/rytdl.exe)
#   OUT          output bundle path                 (default ytdl-rmcp.mcpb)
#   DXT_OUT      legacy .dxt alias path             (default derived from OUT)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() { echo "[build-mcpb] $*" >&2; }
fail() {
  echo "[build-mcpb] error: $*" >&2
  exit 1
}

LINUX_BIN="${LINUX_BIN:-target/release/rytdl}"
WINDOWS_BIN="${WINDOWS_BIN:-target/x86_64-pc-windows-msvc/release/rytdl.exe}"
OUT="${OUT:-ytdl-rmcp.mcpb}"
DXT_OUT="${DXT_OUT:-${OUT%.mcpb}.dxt}"
MANIFEST="mcpb/manifest.json"
case "$OUT" in
  /*) OUT_PATH="$OUT" ;;
  *) OUT_PATH="$ROOT/$OUT" ;;
esac

[ -f "$MANIFEST" ] || fail "missing $MANIFEST"
[ -f "$LINUX_BIN" ] || fail "linux binary not found: $LINUX_BIN (set LINUX_BIN)"
[ -f "$WINDOWS_BIN" ] || fail "windows binary not found: $WINDOWS_BIN (set WINDOWS_BIN)"

build_dir="$(mktemp -d)"
trap 'rm -rf "$build_dir"' EXIT

mkdir -p "$build_dir/server"
cp "$MANIFEST" "$build_dir/manifest.json"
cp "$LINUX_BIN" "$build_dir/server/rytdl"
cp "$WINDOWS_BIN" "$build_dir/server/rytdl.exe"
chmod +x "$build_dir/server/rytdl"
log "staged manifest + linux/windows binaries"

# The mcpb CLI validates the manifest against the official schema.
npx -y @anthropic-ai/mcpb validate "$build_dir/manifest.json"

rm -f "$OUT"
(cd "$build_dir" && zip -0 -r "$OUT_PATH" manifest.json server >/dev/null)
if unzip -lv "$OUT" | awk 'NR > 3 && $2 ~ /^Defl/ { bad = 1 } END { exit bad }'; then
  log "archive entries stored without deflate compression"
else
  fail "$OUT contains deflated entries; Claude Desktop for Windows may silently ignore it"
fi

sha256sum "$OUT" > "$OUT.sha256"
cp "$OUT" "$DXT_OUT"
sha256sum "$DXT_OUT" > "$DXT_OUT.sha256"
log "wrote $OUT, $DXT_OUT, and checksums"
