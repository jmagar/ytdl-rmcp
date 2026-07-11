---
date: 2026-06-11 01:59:00 EST
repo: git@github.com:jmagar/ytdl-rmcp.git
branch: codex/youtube-search-mcp-ui
head: 3c6f427
working directory: /home/jmagar/workspace/ytdl-rmcp/.worktrees/youtube-search-mcp-ui
worktree: /home/jmagar/workspace/ytdl-rmcp/.worktrees/youtube-search-mcp-ui
pr: "#2 Add YouTube search MCP tool and UI (https://github.com/jmagar/ytdl-rmcp/pull/2)"
---

# YouTube search MCP tool and UI session

## User Request

Add YouTube search support to `ytdl-rmcp` as both a regular MCP tool and an MCP-UI/MCP App component, using Aurora design guidance. The user also asked that the feature remain a regular tool, and later asked to use `mcporter` for MCP smoke testing.

## Session Overview

Implemented `youtube_search` and `youtube_search_ui`, added an embedded Aurora-styled MCP App, vendored the official MCP Apps browser bridge, opened PR #2, ran multiple review waves, addressed all actionable findings, and verified the branch locally and through GitHub CI.

## Sequence of Events

1. Created an isolated worktree at `/home/jmagar/workspace/ytdl-rmcp/.worktrees/youtube-search-mcp-ui` on branch `codex/youtube-search-mcp-ui`.
2. Implemented YouTube search through `yt-dlp` using `ytsearchN:<query>`, added search input/output models, and exposed the regular `youtube_search` MCP tool.
3. Added `youtube_search_ui` plus a resource at `ui://ytdl-rmcp/youtube-search.html` with `text/html;profile=mcp-app`, Aurora styling, and controls for Probe, Audio, and Video actions.
4. Corrected MCP App metadata to nested `_meta.ui`, vendored the MCP Apps browser bridge into `assets/ext-apps-vendored.js`, and removed external `esm.sh` loading from the app.
5. Ran review agents and PR toolkit passes, then fixed surfaced issues: UI tool errors no longer render as empty results, action responses are visible, parser output is stricter, raw yt-dlp IDs become YouTube watch URLs, and the UI tool advertises structured output schema.
6. Verified locally with Rust tests, clippy, `mcporter`, Node syntax checks, and a headless Chrome screenshot; pushed PR #2.

## Key Findings

- MCP App metadata must be attached to the advertised tool definition as nested `_meta.ui.resourceUri`, not only to the call result.
- The UI resource must use `text/html;profile=mcp-app` and declare CSP through `_meta.ui.csp`.
- `mcporter` can list and call the ad-hoc stdio server and read resources from a temporary `mcpServers` config, but its schema output did not display `_meta` or `outputSchema`; direct Rust router tests were added for those contracts.
- Tool-level MCP failures are normal `CallToolResult` values with `isError`, so the app must inspect returned results instead of relying only on thrown errors.
- yt-dlp search entries can expose extractor-local `url` values; the parser now prefers `webpage_url`, accepts only HTTP(S) raw URLs, and otherwise synthesizes `https://www.youtube.com/watch?v=<id>`.

## Technical Decisions

- Used `yt-dlp --dump-single-json ytsearchN:<query>` to avoid adding a YouTube Data API dependency or credential requirement.
- Kept `youtube_search` as a text-first tool with markdown or JSON responses, while `youtube_search_ui` returns text fallback plus `structured_content` and MCP App metadata.
- Vendored the official MCP Apps browser bridge to avoid runtime dependence on an external CDN and to keep the resource CSP tight.
- Kept the UI in a static embedded HTML file so the Rust binary remains a single distributable MCP server.
- Added targeted tests rather than a broad browser test harness inside the Rust crate; the rendered screenshot and Node syntax check cover the app asset separately.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `.gitignore` | - | Ignore local `.worktrees/` checkouts. | Commit `8e986e2` |
| modified | `README.md` | - | Document `youtube_search` and `youtube_search_ui`. | Commit `38a37b5` |
| created | `assets/ext-apps-vendored.js` | - | Vendored MCP Apps browser bridge. | Commit `fce84fd` |
| created | `assets/youtube-search-app.html` | - | Embedded Aurora-styled MCP App UI. | Commits `3a68125`, `7e966c8`, `eaff6f1`, `3c6f427` |
| created | `docs/superpowers/artifacts/youtube-search-ui-render.png` | - | Headless Chrome UI proof screenshot. | Screenshot regenerated after UI changes |
| modified | `skills/ytdl/SKILL.md` | - | Document four MCP tools and examples. | Commits `38a37b5`, `eaff6f1` |
| modified | `src/downloader.rs` | - | Add YouTube search execution, parsing, and URL normalization. | Commit `3c6f427` |
| modified | `src/downloader_tests.rs` | - | Add search parser tests and edge cases. | 51-test suite passed |
| modified | `src/main.rs` | - | Wire resource-serving module into the binary. | PR diff |
| modified | `src/mcp.rs` | - | Expose search tools, resources, metadata, and output schema. | Router contract test passed |
| created | `src/mcp_tests.rs` | - | Assert `youtube_search_ui` app metadata and output schema. | `cargo test` passed |
| modified | `src/model.rs` | - | Add search input/result/payload types. | PR diff |
| modified | `src/model_tests.rs` | - | Cover search input defaults and limit clamping. | `cargo test` passed |
| created | `src/search_app.rs` | - | Serve MCP App resource and UI metadata. | `cargo test search_app` passed |
| created | `src/search_app_tests.rs` | - | Cover resource MIME, CSP, vendored bridge, and metadata helper. | `cargo test search_app` passed |
| modified | `src/service.rs` | - | Add search orchestration and split formatting. | PR diff |
| created | `src/service/format.rs` | - | Centralize payload rendering helpers. | Full test suite passed |
| modified | `src/service_tests.rs` | - | Add fake yt-dlp search orchestration tests. | `cargo test` passed |

## Beads Activity

No bead activity observed. `bd list --all --sort updated --reverse --limit 100 --json` returned `[]`, and `.beads/interactions.jsonl` was absent.

## Repository Maintenance

### Plans

Checked `docs/plans` and `docs/superpowers/plans`; no plan files were present in the worktree at closeout. No completed plans were moved.

### Beads

Checked beads as described above. No beads existed in this repo/worktree, so no bead state was changed.

### Worktrees and branches

`git worktree list --porcelain` showed the main checkout at `/home/jmagar/workspace/ytdl-rmcp` on `main` and the active feature worktree at `.worktrees/youtube-search-mcp-ui` on `codex/youtube-search-mcp-ui`. No worktrees or branches were removed because the PR branch is active and unmerged.

### Stale docs

Updated `README.md` and `skills/ytdl/SKILL.md` to describe the new tools. Also fixed stale wording in `src/mcp.rs` that still referred to two tools.

### Transparency

The main checkout branch `main` is ahead of `origin/main` by `8e986e2 chore: ignore local worktrees`; that commit was intentionally created earlier to ignore local worktrees. No cleanup was attempted for unrelated main-checkout state.

## Tools and Skills Used

- Shell commands: git, cargo, gh, node, perl, Chrome headless, and mcporter for build/test/PR/MCP verification.
- File tools: `apply_patch` for code/session edits and `view_image` for screenshot inspection.
- Skills: `vibin:work-it`, `superpowers:writing-plans`, `vibin:aurora-design-system`, `lavra:frontend-design`, `build-web-apps:frontend-app-builder`, `testing:mcporter`, and `vibin:save-to-md`.
- Subagents: implementation worker, Lavra review agents, simplifiers, and PR review toolkit agents.
- External CLIs: `mcporter` for ad-hoc stdio MCP list/call/resource smoke tests; `gh` for PR checks/comments.
- Browser tooling: `/usr/bin/google-chrome --headless=new` for the UI screenshot because local Playwright module/browser setup was unavailable.

## Commands Executed

| command | result |
| --- | --- |
| `cargo build` | Baseline build passed before implementation. |
| `cargo test` | Final local suite passed: 51 tests. |
| `cargo fmt --all --check` | Passed. |
| `cargo clippy --all-targets -- -D warnings` | Passed. |
| `node --check /tmp/youtube-search-app-inline.js` | Embedded app script parsed successfully. |
| `mcporter list --config /tmp/ytdl-rmcporter-config.json ytdl-local --schema --json` | Listed `youtube_download`, `youtube_probe`, `youtube_search`, and `youtube_search_ui`. |
| `mcporter call --config /tmp/ytdl-rmcporter-config.json ytdl-local.youtube_search ...` | Returned fake YouTube search result URL. |
| `mcporter resource --config /tmp/ytdl-rmcporter-config.json ytdl-local ui://ytdl-rmcp/youtube-search.html --output raw` | Resource contained `window.McpExtApps`, `text/html;profile=mcp-app`, and no `https://esm.sh`. |
| `/usr/bin/google-chrome --headless=new --screenshot=docs/superpowers/artifacts/youtube-search-ui-render.png file:///tmp/youtube-search-app-expanded.html?demo` | Produced a nonblank UI screenshot. |
| `gh pr checks 2 --repo jmagar/ytdl-rmcp` | Earlier run passed; new run was pending at session-log write time after final push. |

## Errors Encountered

- Initial handmade stdio JSON-RPC smoke harness hung; switched to `mcporter`.
- First `mcporter resource` attempt used ad-hoc flags unsupported by the `resource` subcommand; fixed by writing a temporary `mcpServers` config in `/tmp`.
- `mcporter` schema display did not preserve `_meta` or `outputSchema`; added Rust router tests for those contracts.
- A fake executable test hit `Text file busy` once; fixed by writing through `File`, `sync_all`, and `drop` before chmod/execute.
- Headless Chrome emitted DBus warnings in the desktop environment, but it still wrote the screenshot successfully.
- CodeRabbit left a rate-limit/spending-cap comment instead of actionable code feedback.

## Behavior Changes (Before/After)

| area | before | after |
| --- | --- | --- |
| Search tool | No YouTube search MCP tool. | `youtube_search` returns markdown or JSON search results. |
| Search UI | No MCP App resource. | `youtube_search_ui` renders an interactive Aurora-styled search app in UI-capable hosts. |
| UI errors | Tool errors could appear as empty results. | `isError` results and invalid payloads are surfaced to the user. |
| Result actions | Probe/Audio/Video clicks discarded tool responses. | Action success or error text is displayed above results. |
| Parser | Missing `entries` looked like empty results and raw IDs could leak as URLs. | Missing `entries` errors explicitly and IDs are normalized to watch URLs. |
| MCP contract | UI app metadata and output shape were implicit. | UI tool advertises app metadata and output schema; tests assert both. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo fmt --all --check` | Rust formatting clean. | Passed. | pass |
| `cargo test` | All tests pass. | 51 passed, 0 failed. | pass |
| `cargo clippy --all-targets -- -D warnings` | No clippy warnings. | Passed. | pass |
| `git diff --check` | No whitespace errors. | Passed. | pass |
| `node --check /tmp/youtube-search-app-inline.js` | UI script parses. | Passed. | pass |
| `mcporter list/call/resource` | Tools listed, search callable, UI resource readable. | Passed with fake yt-dlp and temp config. | pass |
| Headless Chrome screenshot | Demo UI renders nonblank and framed. | Passed; screenshot inspected. | pass |
| `gh pr checks 2 --repo jmagar/ytdl-rmcp` | CI green. | Previous run green; final push run pending when this note was written. | warn |

## Risks and Rollback

The search implementation depends on yt-dlp search JSON shape and YouTube extractor behavior. Parser tests now cover schema drift and ID normalization, but live extractor changes can still occur. Rollback path is to revert the feature commits on `codex/youtube-search-mcp-ui` or disable use of `youtube_search_ui` while retaining existing download/probe tools.

## Decisions Not Taken

- Did not add YouTube Data API integration because it would require credentials and quota management.
- Did not add a full jsdom/browser test harness inside the Rust crate; the static HTML is covered by Node syntax checks, resource tests, mcporter resource reads, and headless Chrome screenshot proof.
- Did not split `SearchUiInput` from `SearchInput`; the shared shape keeps UI invocation aligned with regular search, while `youtube_search_ui` still provides structured content for UI hosts.

## References

- PR #2: https://github.com/jmagar/ytdl-rmcp/pull/2
- MCP Apps/ext-apps package reference cloned under `/tmp/mcp-ext-apps-ytdl`
- mcporter CLI help and `testing:mcporter` skill guidance
- Aurora design tokens from the invoked Aurora design-system skill

## Open Questions

- Final GitHub CI for commit `3c6f427` was pending when this session note was written.

## Next Steps

1. Wait for final PR checks to complete after the session-log commit push.
2. Re-run `gh pr checks 2 --repo jmagar/ytdl-rmcp` and confirm check, packaging, cross-build, GitGuardian, and CodeRabbit remain green.
3. Merge PR #2 when the final checks are green and no new actionable review comments appear.
