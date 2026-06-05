#!/usr/bin/env bash
# MCP server entry point for the Claude Code plugin. Ensures the ytdl-mcp
# binary is present (a no-op after first run), then hands stdin/stdout to it
# for the JSON-RPC transport.
set -euo pipefail

DATA="${CLAUDE_PLUGIN_DATA:?CLAUDE_PLUGIN_DATA not set}"
HERE="$(cd "$(dirname "$0")" && pwd)"

# The SessionStart hook normally pre-fetches the binary; doing it here too makes
# the server self-sufficient. fetch-binary.sh writes only to stderr.
"$HERE/fetch-binary.sh"

exec "$DATA/bin/ytdl-mcp" serve
