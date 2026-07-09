---
date: 2026-06-12 10:39:48 EST
repo: git@github.com:jmagar/ytdl-rmcp.git
branch: codex/metadata-playlist-sync
head: c6a0d62
session id: e4558680-1081-4d6a-8417-e10f29ef0281
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-ytdl-rmcp/e4558680-1081-4d6a-8417-e10f29ef0281.jsonl
working directory: /home/jmagar/workspace/ytdl-rmcp
worktree: /home/jmagar/workspace/ytdl-rmcp c6a0d62 [codex/metadata-playlist-sync]
---

# ytdl metadata and Plex playlist session

## User Request

Make ytdl-rmcp automatically add downloaded tracks to the Plex playlist containing existing yt-dlp downloads, improve embedded metadata, clean and normalize title metadata, and look into MusicBrainz/AcoustID for future canonical metadata matching.

## Session Overview

This session created the Plex audio playlist `yt-dlp Downloads`, populated it with all 118 currently indexed yt-dlp audio tracks from tootie, added automatic default playlist targeting to ytdl-rmcp, preserved richer yt-dlp sidecar metadata, added default-on title metadata cleanup, and documented the recommended MusicBrainz/AcoustID retagging path.

## Sequence of Events

1. Inspected current ytdl-rmcp logs and archive state, confirming `downloads.jsonl` was new and `archive-audio.txt` contained yt-dlp archive IDs without per-item timestamps.
2. Inventoried tootie's yt-dlp music folder and Plex library section `9`, then created and populated the Plex playlist `yt-dlp Downloads` with 118 tracks.
3. Added ytdl-rmcp default playlist behavior so Plex URL/token imply `yt-dlp Downloads` unless overridden.
4. Added sidecar metadata preservation and playlist-title album parsing to yt-dlp arguments.
5. Added configurable title metadata cleanup via `YTDLP_CLEAN_METADATA`, defaulting on, plus docs and tests.
6. Researched MusicBrainz/AcoustID integration constraints and recorded an opt-in retagging plan.
7. Started quick-push, created branch `codex/metadata-playlist-sync`, and bumped version `0.5.0` to `0.6.0`.

## Key Findings

- Plex library section `9` (`nugs.net`) indexes `/data/music`, including `/data/music/yt-dlp`.
- The existing yt-dlp folder on tootie is `/mnt/user/data/media/music/yt-dlp`, and Plex read-back showed 118 playlist items after population.
- `YTDLP_PLEX_URL` and `YTDLP_PLEX_TOKEN` are sufficient for the new default playlist behavior; `YTDLP_PLEX_PLAYLIST` still overrides the target.
- `fpcalc` is not installed on this host, so AcoustID fingerprinting needs either bootstrap/download support or an in-process Rust fingerprinting path.
- There is no beads database in this repo (`bd` reported no beads database found), and no `docs/plans` files were present.

## Technical Decisions

- Default playlist name is `yt-dlp Downloads`, matching the Plex playlist created from the existing library.
- Title cleanup is default-on but configurable with `YTDLP_CLEAN_METADATA=0` to preserve source titles exactly when needed.
- Sidecar metadata (`.info.json`, thumbnails, descriptions) is preserved because it improves future indexing/retagging without guessing.
- MusicBrainz/AcoustID is documented as an opt-in future flow because it needs an API key, client identification, fingerprint generation, confidence scoring, and MusicBrainz rate limiting.
- Probe logic was split into `src/downloader/probe.rs` to keep `src/downloader.rs` below the repo's 500-line limit after metadata changes.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `.claude-plugin/plugin.json` | — | Add plugin defaults for Plex playlist and metadata cleanup; bump version to `0.6.0`. | `git diff --stat` |
| modified | `.mcp.json` | — | Pass `YTDLP_CLEAN_METADATA` from plugin user config into the MCP server environment. | `git diff --stat` |
| modified | `Cargo.toml` | — | Bump package version to `0.6.0`. | `cargo check` reported `ytdl-rmcp v0.6.0` |
| modified | `Cargo.lock` | — | Record the new Rust package version. | `rg` showed `version = "0.6.0"` |
| modified | `README.md` | — | Document automatic Plex playlist sync, sidecar metadata, metadata cleanup, and MusicBrainz/AcoustID follow-up. | `git diff --stat` |
| modified | `gemini-extension.json` | — | Expose version and metadata cleanup config for Gemini extension installs. | `git diff --stat` |
| modified | `skills/ytdl/SKILL.md` | — | Document metadata cleanup and Plex playlist default for future ytdl workflows. | `git diff --stat` |
| created | `docs/musicbrainz-acoustid.md` | — | Record MusicBrainz/AcoustID constraints and recommended implementation plan. | new file in `git status` |
| modified | `src/bootstrap_tests.rs` | — | Add `clean_metadata` to test config literals. | `cargo test` |
| modified | `src/config.rs` | — | Add `DEFAULT_PLEX_PLAYLIST` and `clean_metadata` configuration. | `cargo test` |
| modified | `src/config_tests.rs` | — | Test Plex playlist default and metadata cleanup opt-out. | `cargo test` |
| modified | `src/downloader.rs` | — | Preserve sidecars, add metadata cleanup args, and re-export probe module. | `cargo test`; yt-dlp simulate check |
| created | `src/downloader/probe.rs` | — | Move probe implementation out of `src/downloader.rs`. | line-count check |
| modified | `src/downloader_tests.rs` | — | Test sidecar and cleanup yt-dlp arguments. | `cargo test` |
| modified | `src/plex_tests.rs` | — | Add `clean_metadata` to config literal. | `cargo test` |
| modified | `src/service.rs` | — | Pass cleanup setting into downloader fetch options. | `cargo test` |
| modified | `src/service_tests.rs` | — | Add `clean_metadata` to test config literal. | `cargo test` |

## Beads Activity

No bead activity observed. `bd list --all --sort updated --reverse --limit 100 --json` returned no usable issue list because the repo has no beads database.

## Repository Maintenance

### Plans

No plan files were found under `docs/plans`; no plan cleanup was performed.

### Beads

`bd` reported no beads database found, so no beads were created, edited, or closed.

### Worktrees and branches

`git worktree list --porcelain` showed only `/home/jmagar/workspace/ytdl-rmcp`, now on `refs/heads/codex/metadata-playlist-sync`. No worktrees or branches were pruned during quick-push.

### Stale docs

README, Gemini extension metadata, plugin metadata, and the ytdl skill doc were updated to match the new behavior. A new MusicBrainz/AcoustID note records future work instead of leaving it only in chat.

### Transparency

The save-to-md maintenance pass was constrained to read-only checks and documentation because quick-push requires avoiding unrelated cleanup before staging all worktree changes.

## Tools and Skills Used

- Shell commands: repo inspection, Plex API calls, tootie SSH inventory, Cargo verification, version sync, and git operations.
- Skills: `vibin:yt-dlp`, `arrs:plex`, `vibin:quick-push`, and `vibin:save-to-md` workflow guidance.
- Web research: MusicBrainz API, AcoustID web service, and crate discovery for `musicbrainz_rs`, Chromaprint, and tag-writing options.
- External CLIs: `cargo`, `yt-dlp`, `ssh`, `curl`, `jq`, `rg`, `bd`, and `gh`.

## Commands Executed

| command | result |
| --- | --- |
| `ssh tootie 'find /mnt/user/data/media/music/yt-dlp ...'` | Found 118 files under the real yt-dlp music folder. |
| Plex playlist creation/update script | Created `yt-dlp Downloads` playlist id `822064` and added 118 items. |
| `cargo test` | Passed 62 tests after metadata cleanup work. |
| `cargo clippy --all-targets -- -D warnings` | Passed. |
| `cargo fmt --all --check` | Passed. |
| yt-dlp simulate command with cleanup flags | Accepted flags and printed the Goose title. |
| `cargo build --release` | Built `/home/jmagar/workspace/ytdl-rmcp/target/release/ytdl-rmcp`. |
| `cargo check` | Passed after version bump and updated Cargo.lock to `0.6.0`. |

## Errors Encountered

- A broad tootie media scan was too slow and was narrowed to known yt-dlp paths.
- Python Plex API requests initially returned 403 until a normal User-Agent was set.
- A first targeted Cargo test command used two filters, which Cargo rejected; the full test suite was then run successfully.
- An earlier assistant response prematurely stopped after branch creation; the quick-push continued from the created branch.

## Behavior Changes (Before/After)

| area | before | after |
| --- | --- | --- |
| Plex playlist sync | Downloads only added to Plex when a playlist was explicitly configured or passed per call. | Plex URL/token now imply the default `yt-dlp Downloads` playlist. |
| Metadata preservation | Embedded metadata/thumbnail were used, but source sidecars were not preserved. | `.info.json`, thumbnails, and descriptions are preserved beside media. |
| Title metadata | YouTube suffixes and channel handles could remain in embedded title tags. | Common YouTube title noise is stripped by default, with `YTDLP_CLEAN_METADATA=0` opt-out. |
| Probe module layout | Probe code lived inside `src/downloader.rs`. | Probe code lives in `src/downloader/probe.rs`, keeping `src/downloader.rs` under 500 lines. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test` | All tests pass. | 62 passed. | pass |
| `cargo clippy --all-targets -- -D warnings` | No warnings/errors. | Finished successfully. | pass |
| `cargo fmt --all --check` | Formatted code. | No diff reported. | pass |
| yt-dlp simulate with metadata cleanup flags | Real yt-dlp accepts generated flags. | Command exited 0 and printed title. | pass |
| `cargo build --release` | Release binary rebuilds. | Built `ytdl-rmcp 0.5.0` before version bump; `cargo check` later verified `0.6.0`. | pass |
| `cargo check` | Version bump compiles and updates lockfile. | Checked `ytdl-rmcp v0.6.0`. | pass |

## Risks and Rollback

Title cleanup regexes may remove words that a user wants preserved in rare cases. Roll back by setting `YTDLP_CLEAN_METADATA=0`, or revert the metadata cleanup change in `src/downloader.rs` and config wiring.

Automatic Plex playlist sync requires Plex URL/token in the ytdl-rmcp environment. If those are absent, downloads still succeed but playlist updates do not run.

## Decisions Not Taken

- Did not implement MusicBrainz/AcoustID tag writing immediately because it needs AcoustID credentials, fingerprinting support, MusicBrainz user-agent/rate-limit compliance, and confidence scoring.
- Did not use SponsorBlock chapter removal; it is better suited to video-specific behavior and should not alter audio downloads by default.
- Did not split the existing oversized `src/service_tests.rs`; it predates this session and is unrelated to the metadata/playlist change.

## References

- MusicBrainz API: https://musicbrainz.org/doc/MusicBrainz_API
- AcoustID web service: https://acoustid.org/webservice
- musicbrainz_rs docs: https://docs.rs/musicbrainz_rs
- MusicBrainz/AcoustID plan: `docs/musicbrainz-acoustid.md`

## Open Questions

- Which AcoustID application key should ytdl-rmcp use, and where should it be configured?
- Should fingerprinting use a bootstrapped `fpcalc` binary or a pure-Rust Chromaprint/Symphonia pipeline?
- What confidence threshold should be required before writing canonical MusicBrainz tags?

## Next Steps

- Finish quick-push by committing this session document, staging the remaining changes, committing the feature work, and pushing `codex/metadata-playlist-sync`.
- Add a read-only `youtube_identify` tool for MusicBrainz/AcoustID candidate matching before adding tag-writing.
- Consider a future cleanup to split `src/service_tests.rs` below the repo's 500-line convention.
