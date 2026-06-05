#!/usr/bin/env bash
# Idempotently download the ytdl-mcp release binary into the plugin's persistent
# data dir. Safe to run repeatedly: only downloads when the binary is missing.
# All output goes to stderr so it never pollutes the MCP stdio channel.
set -euo pipefail

DATA="${CLAUDE_PLUGIN_DATA:?CLAUDE_PLUGIN_DATA not set}"
BIN_DIR="$DATA/bin"
BIN="$BIN_DIR/ytdl-mcp"
REPO="jmagar/ytdl-mcp"

log() { echo "[ytdl-mcp] $*" >&2; }

if [ -x "$BIN" ]; then
  exit 0
fi

mkdir -p "$BIN_DIR"

os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Linux/x86_64) asset="ytdl-mcp-linux-x86_64" ;;
  *)
    log "no prebuilt binary for $os/$arch. Download or build from https://github.com/$REPO/releases and place it at $BIN"
    exit 1
    ;;
esac

url="https://github.com/$REPO/releases/latest/download/$asset"

fetch() { # fetch <url> <out>; prints to stdout via -O- when out is "-"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2"
  else
    wget -qO "$2" "$1"
  fi
}

# download to a temp file, verify, then atomically move into place
tmp="$BIN.part"
log "downloading $asset"
fetch "$url" "$tmp"

# Verify against the release's published checksum when present (releases built
# by .github/workflows/release.yml publish <asset>.sha256). Best-effort: if a
# release has no checksum, warn and proceed rather than hard-fail.
expected=$(fetch "$url.sha256" - 2>/dev/null | awk '{print $1}') || true
if [ -n "$expected" ]; then
  actual=$(sha256sum "$tmp" | awk '{print $1}')
  if [ "$expected" != "$actual" ]; then
    rm -f "$tmp"
    log "checksum mismatch for $asset (expected $expected, got $actual) — refusing to install"
    exit 1
  fi
  log "checksum verified"
else
  log "no published checksum for $asset; skipping verification"
fi

chmod 0755 "$tmp"
mv -f "$tmp" "$BIN"
log "ready: $BIN"
