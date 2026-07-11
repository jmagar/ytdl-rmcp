#!/usr/bin/env bash
# Validate the non-Rust release surface shipped with the Claude plugin and
# Gemini extension. CI sets REQUIRE_SHELLCHECK=1 so missing ShellCheck is a
# hard failure there; local runs use ShellCheck when available.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() { echo "[check-packaging] $*" >&2; }
fail() {
  echo "[check-packaging] error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

require_cmd jq

json_files=(
  ".claude-plugin/plugin.json"
  ".mcp.json"
  "gemini-extension.json"
  "hooks/hooks.json"
  "mcpb/manifest.json"
)

for file in "${json_files[@]}"; do
  [ -f "$file" ] || fail "missing JSON file: $file"
  jq empty "$file"
done
log "JSON syntax ok"

shell_scripts=()
while IFS= read -r -d '' file; do
  shell_scripts+=("$file")
done < <(find scripts -maxdepth 1 -type f -name "*.sh" -print0 | sort -z)

[ "${#shell_scripts[@]}" -gt 0 ] || fail "no shell scripts found"
for file in "${shell_scripts[@]}"; do
  bash -n "$file"
done
log "shell syntax ok"

if command -v shellcheck >/dev/null 2>&1; then
  shellcheck "${shell_scripts[@]}"
  log "shellcheck ok"
elif [ "${REQUIRE_SHELLCHECK:-}" = "1" ]; then
  fail "shellcheck is required when REQUIRE_SHELLCHECK=1"
else
  log "shellcheck not found; skipped"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

jq -r '.userConfig | keys[]' .claude-plugin/plugin.json | sort -u > "$tmp_dir/plugin_keys"
jq -r '.. | strings | capture("\\$\\{user_config\\.(?<key>[A-Za-z0-9_]+)\\}")? | .key' .mcp.json \
  | sort -u > "$tmp_dir/mcp_refs"
jq -r '.user_config | keys[]' .mcp.json | sort -u > "$tmp_dir/mcp_user_config_keys"

[ -s "$tmp_dir/plugin_keys" ] || fail ".claude-plugin/plugin.json has no userConfig keys"
[ -s "$tmp_dir/mcp_refs" ] || fail ".mcp.json has no user_config references"
[ -s "$tmp_dir/mcp_user_config_keys" ] || fail ".mcp.json has no user_config keys"

missing_plugin_keys="$(comm -23 "$tmp_dir/mcp_refs" "$tmp_dir/plugin_keys")"
if [ -n "$missing_plugin_keys" ]; then
  fail ".mcp.json references undeclared user_config keys: ${missing_plugin_keys//$'\n'/, }"
fi

unused_plugin_keys="$(comm -13 "$tmp_dir/mcp_refs" "$tmp_dir/plugin_keys")"
if [ -n "$unused_plugin_keys" ]; then
  fail ".claude-plugin/plugin.json userConfig keys are not mapped in .mcp.json: ${unused_plugin_keys//$'\n'/, }"
fi

drift_mcp_user_config_keys="$(comm -3 "$tmp_dir/plugin_keys" "$tmp_dir/mcp_user_config_keys")"
if [ -n "$drift_mcp_user_config_keys" ]; then
  fail ".mcp.json user_config keys differ from plugin.json userConfig keys: ${drift_mcp_user_config_keys//$'\n'/, }"
fi

missing_mcp_defaults="$(jq -r '.user_config | to_entries[] | select(.value | has("default") | not) | .key' .mcp.json)"
if [ -n "$missing_mcp_defaults" ]; then
  fail ".mcp.json user_config keys need defaults for raw MCP imports: ${missing_mcp_defaults//$'\n'/, }"
fi
log "Claude userConfig mapping ok"

jq -r '.mcpServers."ytdl-rmcp".env | keys[]' .mcp.json | sort -u > "$tmp_dir/mcp_env_vars"
{
  cat "$tmp_dir/mcp_env_vars"
  printf '%s\n' "YTDLP_LOG"
} | sort -u > "$tmp_dir/readme_env_vars"

grep -q '| Var | Required? | Default | Used by | Meaning |' README.md \
  || fail "README.md env table must include a Required? column"

missing_readme_env="$(
  while IFS= read -r env_var; do
    grep -Fq "\`$env_var\`" README.md || printf '%s\n' "$env_var"
  done < "$tmp_dir/readme_env_vars"
)"
if [ -n "$missing_readme_env" ]; then
  fail "README.md is missing env var documentation: ${missing_readme_env//$'\n'/, }"
fi
log "README env coverage ok"

cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
npm_version="$(jq -r '.version' packages/ytdl-rmcp/package.json)"
gemini_version="$(jq -r '.version' gemini-extension.json)"
mcpb_version="$(jq -r '.version' mcpb/manifest.json)"
if [ "$cargo_version" != "$npm_version" ] \
  || [ "$cargo_version" != "$gemini_version" ] \
  || [ "$cargo_version" != "$mcpb_version" ]; then
  fail "version drift: Cargo=$cargo_version npm=$npm_version gemini=$gemini_version mcpb=$mcpb_version"
fi
log "version sync ok"

jq -e '
  (.settings | type == "array")
  and all(.settings[];
    (.name | type == "string" and length > 0)
    and (.description | type == "string" and length > 0)
    and (.envVar | type == "string" and length > 0)
  )
' gemini-extension.json >/dev/null || fail "gemini-extension.json settings must include name, description, and envVar strings"

jq -r '.settings[].envVar' gemini-extension.json | sort > "$tmp_dir/gemini_env_vars"
duplicate_gemini_env="$(uniq -d "$tmp_dir/gemini_env_vars")"
if [ -n "$duplicate_gemini_env" ]; then
  fail "gemini-extension.json has duplicate envVar entries: ${duplicate_gemini_env//$'\n'/, }"
fi

invalid_gemini_env="$(awk '!/^(YTDLP_|FFMPEG_|FPCALC_PATH$)/ { print }' "$tmp_dir/gemini_env_vars")"
if [ -n "$invalid_gemini_env" ]; then
  fail "gemini-extension.json has unexpected envVar names: ${invalid_gemini_env//$'\n'/, }"
fi

unmapped_gemini_env="$(comm -23 "$tmp_dir/gemini_env_vars" "$tmp_dir/mcp_env_vars")"
if [ -n "$unmapped_gemini_env" ]; then
  fail "gemini-extension.json envVars are not present in .mcp.json env mapping: ${unmapped_gemini_env//$'\n'/, }"
fi
log "Gemini env mapping ok"

# MCP bundle (.mcpb) manifest: a binary-type server whose user_config and env
# mapping must stay aligned with the canonical Claude plugin userConfig keys.
mcpb_manifest="mcpb/manifest.json"
[ -f "$mcpb_manifest" ] || fail "missing $mcpb_manifest"

jq -e '.server.type == "binary"' "$mcpb_manifest" >/dev/null \
  || fail "mcpb/manifest.json server.type must be \"binary\""
jq -e '.server.mcp_config.command | startswith("${__dirname}/")' "$mcpb_manifest" >/dev/null \
  || fail "mcpb/manifest.json binary command must use \${__dirname}"
jq -e '.server.mcp_config.platform_overrides.win32.command | startswith("${__dirname}/")' "$mcpb_manifest" >/dev/null \
  || fail "mcpb/manifest.json win32 binary command must use \${__dirname}"

jq -r '.user_config | keys[]' "$mcpb_manifest" | sort -u > "$tmp_dir/mcpb_keys"
jq -r '.server.mcp_config.env | .. | strings
  | capture("\\$\\{user_config\\.(?<key>[A-Za-z0-9_]+)\\}")? | .key' "$mcpb_manifest" \
  | sort -u > "$tmp_dir/mcpb_env_refs"

[ -s "$tmp_dir/mcpb_keys" ] || fail "mcpb/manifest.json has no user_config keys"

missing_mcpb_keys="$(comm -23 "$tmp_dir/mcpb_env_refs" "$tmp_dir/mcpb_keys")"
if [ -n "$missing_mcpb_keys" ]; then
  fail "mcpb/manifest.json env references undeclared user_config keys: ${missing_mcpb_keys//$'\n'/, }"
fi

unused_mcpb_keys="$(comm -13 "$tmp_dir/mcpb_env_refs" "$tmp_dir/mcpb_keys")"
if [ -n "$unused_mcpb_keys" ]; then
  fail "mcpb/manifest.json user_config keys are not mapped in mcp_config.env: ${unused_mcpb_keys//$'\n'/, }"
fi

missing_mcpb_defaults="$(jq -r '
  .server.mcp_config.env | .. | strings
  | capture("\\$\\{user_config\\.(?<key>[A-Za-z0-9_]+)\\}")? | .key
' "$mcpb_manifest" | sort -u | while IFS= read -r key; do
  jq -e --arg key "$key" '.user_config[$key] | has("default")' "$mcpb_manifest" >/dev/null \
    || printf '%s\n' "$key"
done)"
if [ -n "$missing_mcpb_defaults" ]; then
  fail "mcpb/manifest.json env-backed user_config keys need defaults for Desktop installer compatibility: ${missing_mcpb_defaults//$'\n'/, }"
fi

required_mcpb_keys="$(jq -r '.user_config | to_entries[] | select(.value.required == true) | .key' "$mcpb_manifest")"
if [ -n "$required_mcpb_keys" ]; then
  fail "mcpb/manifest.json should avoid required user_config gates during Desktop install: ${required_mcpb_keys//$'\n'/, }"
fi

# Keep the bundle's user_config keys identical to the Claude plugin's set so the
# four distribution surfaces never drift apart.
drift_mcpb_keys="$(comm -3 "$tmp_dir/plugin_keys" "$tmp_dir/mcpb_keys")"
if [ -n "$drift_mcpb_keys" ]; then
  fail "mcpb/manifest.json user_config keys differ from plugin.json userConfig keys: ${drift_mcpb_keys//$'\n'/, }"
fi

mcpb_mcp_semantic_drift="$(
  jq -r '
    .user_config
    | to_entries[]
    | [
        .key,
        (if .key == "plex_playlist" then "__ALLOW_SURFACE_DEFAULT__" else (.value.default // "__MISSING__") end),
        (.value.description // "__MISSING__")
      ]
    | @tsv
  ' .mcp.json | sort > "$tmp_dir/mcp_semantic"
  jq -r '
    .user_config
    | to_entries[]
    | [
        .key,
        (if .key == "plex_playlist" then "__ALLOW_SURFACE_DEFAULT__" else (.value.default // "__MISSING__") end),
        (.value.description // "__MISSING__")
      ]
    | @tsv
  ' "$mcpb_manifest" | sort > "$tmp_dir/mcpb_semantic"
  comm -3 "$tmp_dir/mcp_semantic" "$tmp_dir/mcpb_semantic"
)"
if [ -n "$mcpb_mcp_semantic_drift" ]; then
  fail "mcpb/manifest.json and .mcp.json user_config defaults/descriptions differ"
fi
log "MCP bundle manifest mapping ok"

grep -q 'DXT_OUT' scripts/build-mcpb.sh \
  || fail "scripts/build-mcpb.sh must publish a legacy .dxt alias"
log "MCP bundle legacy alias ok"

release_workflow=".github/workflows/release.yml"
[ -f "$release_workflow" ] || fail "missing $release_workflow"
grep -q 'types: \[published\]' "$release_workflow" \
  || fail "release workflow must run when release-please publishes a GitHub Release"
grep -q 'ytdl-rmcp-x86_64.tar.gz' "$release_workflow" \
  || fail "release workflow must publish the linux npm installer tarball"
grep -q 'ytdl-rmcp-windows-x86_64.tar.gz' "$release_workflow" \
  || fail "release workflow must publish the windows npm installer tarball"
grep -q 'npm publish --provenance --access public ./packages/ytdl-rmcp' "$release_workflow" \
  || fail "release workflow must publish the npm launcher with provenance"
release_tag_expression="$(printf '%s%s' '$' '{{ needs.release-meta.outputs.tag_name }}')"
grep -Fq "tag_name: $release_tag_expression" "$release_workflow" \
  || fail "release uploads must use the computed release tag"
log "release workflow trigger ok"
