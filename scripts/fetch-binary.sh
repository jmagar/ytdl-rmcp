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
log "downloading $asset"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$BIN"
else
  wget -qO "$BIN" "$url"
fi
chmod 0755 "$BIN"
log "ready: $BIN"
