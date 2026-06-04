#!/usr/bin/env bash
# Idempotently build the server's venv in the plugin's persistent data dir.
# Safe to run repeatedly: it only does work when the venv is missing or the
# bundled source changed. All output goes to stderr so it never pollutes the
# MCP stdio (JSON-RPC) channel.
set -euo pipefail

ROOT="${CLAUDE_PLUGIN_ROOT:?CLAUDE_PLUGIN_ROOT not set}"
DATA="${CLAUDE_PLUGIN_DATA:?CLAUDE_PLUGIN_DATA not set}"
VENV="$DATA/venv"
STAMP="$DATA/.installed-from"

log() { echo "[ytdl-mcp] $*" >&2; }

# Reinstall when the venv is absent, its entry point is gone, or the plugin
# was updated to a different root (version bump → new CLAUDE_PLUGIN_ROOT).
need_install=0
if [ ! -x "$VENV/bin/youtube-dl-mcp" ]; then
  need_install=1
elif [ ! -f "$STAMP" ] || [ "$(cat "$STAMP" 2>/dev/null)" != "$ROOT" ]; then
  need_install=1
fi

if [ "$need_install" -eq 0 ]; then
  exit 0
fi

PYTHON="${YTDLP_PYTHON:-python3}"
log "creating venv at $VENV"
"$PYTHON" -m venv "$VENV" >&2
log "installing youtube-dl-mcp + deps (first run can take a minute)"
"$VENV/bin/pip" install --upgrade pip >&2
"$VENV/bin/pip" install -e "$ROOT" >&2
echo "$ROOT" > "$STAMP"
log "ready"
