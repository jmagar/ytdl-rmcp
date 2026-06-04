#!/usr/bin/env bash
# MCP server entry point. Ensures the venv exists (a no-op after first run),
# then hands stdin/stdout to the server for the JSON-RPC transport.
set -euo pipefail

DATA="${CLAUDE_PLUGIN_DATA:?CLAUDE_PLUGIN_DATA not set}"
HERE="$(cd "$(dirname "$0")" && pwd)"

# Ensure the venv is present. The SessionStart hook normally pre-warms this,
# but exec'ing the setup here too makes the server self-sufficient if the hook
# hasn't run yet. setup-venv.sh writes only to stderr.
"$HERE/setup-venv.sh"

exec "$DATA/venv/bin/youtube-dl-mcp"
