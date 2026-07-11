#!/usr/bin/env bash
# MCP server entry point for the Claude Code plugin. Uses an already installed
# ytdl-rmcp binary from PATH, then hands stdin/stdout to it for JSON-RPC.
set -euo pipefail

binary="${YTDL_RMCP_BIN:-ytdl-rmcp}"

if ! command -v "${binary}" >/dev/null 2>&1; then
  printf 'ytdl-rmcp plugin: ytdl-rmcp is not installed or not on PATH.\n' >&2
  printf 'Install ytdl-rmcp separately, then retry the plugin server.\n' >&2
  exit 127
fi

exec "${binary}" serve
