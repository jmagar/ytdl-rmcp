---
date: 2026-06-12 21:55:53 EDT
repo: git@github.com:jmagar/ytdl-rmcp.git
branch: codex/metadata-playlist-sync
head: fb15ce8
working directory: /home/jmagar/workspace/ytdl-rmcp
worktree: /home/jmagar/workspace/ytdl-rmcp fb15ce8 [codex/metadata-playlist-sync]
---

# Container metadata autoretag session

## User Request

Deploy the container on tootie, fix the existing downloaded library metadata,
and make future downloads automatically write better MusicBrainz/AcoustID
metadata when possible.

## Session Overview

This session containerized `ytdl-rmcp`, deployed it to tootie under
`/mnt/cache/compose/ytdl-rmcp`, repaired metadata for the existing yt-dlp music
library, added automatic high-confidence AcoustID/MusicBrainz retagging after
future audio downloads, and bumped the project version from `0.6.0` to `0.7.0`.

## Sequence of Events

1. Built and deployed a Docker Compose stack on tootie using the local
   `ytdl-rmcp:local` image, with `/library`, `/state`, and `/cache` mounts.
2. Ran `youtube_identify` against the existing `/library` contents through
   `mcporter` and wrote high-confidence MusicBrainz tags to 103 of 118 files.
3. Found that `fpcalc` can print valid fingerprints while exiting nonzero with
   `Error decoding audio frame (End of file)`, then updated identification to
   accept valid stdout fingerprints in that case.
4. Added automatic retagging in `youtube_download` before transfer, gated by
   `YTDLP_ACOUSTID_CLIENT_KEY`, so failed or ambiguous metadata lookups do not
   fail the download.
5. Added a GitHub workflow for publishing the container image on pushes to
   `main`, updated container and metadata docs, and refreshed the deployed
   tootie container after rebuilds.
6. Verified the MCP stdio wrapper on tootie with `mcporter`, checked the Plex
   playlist count, and triggered a Plex library refresh for section `9`.
7. Ran quick-push preparation: bumped version `0.6.0` to `0.7.0`, ran
   `cargo check`, and verified remaining `0.6.0` hits were historical session
   notes.

## Key Findings

- The deployed stdio wrapper is `/mnt/cache/compose/ytdl-rmcp/mcp-stdio.sh`, and
  `mcporter` can list and call the server through it.
- The tootie yt-dlp music library has 118 audio files, and the Plex playlist
  `yt-dlp Downloads` also reports 118 items.
- `fpcalc` EOF warnings are recoverable when stdout includes both `DURATION`
  and `FINGERPRINT`; treating the nonzero exit as fatal hid valid matches.
- Automatic retagging should run before transfer so the remote/Plex library sees
  the improved files without a second copy operation.

## Technical Decisions

- Kept automatic retagging best-effort: it reports a `metadata_retag` summary
  but does not fail downloads when AcoustID or MusicBrainz is unavailable.
- Gated auto-retagging on `YTDLP_ACOUSTID_CLIENT_KEY`; no key means the download
  path keeps the old behavior.
- Used the existing `youtube_identify` implementation as the shared tagging path
  instead of duplicating AcoustID/MusicBrainz logic.
- Ran MusicBrainz lookups serially through the existing limiter to respect the
  one-request-per-second expectation.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `.claude-plugin/plugin.json` | — | Add plugin config and keep manifest version in sync. | `git status --short` |
| modified | `.github/workflows/ci.yml` | — | Add container build validation. | `git status --short` |
| created | `.github/workflows/container.yml` | — | Publish GHCR image on pushes to `main`. | `git status --short` |
| created | `.dockerignore` | — | Keep Docker build context small and safe. | `git status --short` |
| created | `Dockerfile` | — | Build runtime image with `ffmpeg`, `fpcalc`, `ssh`, and `rsync`. | `git status --short` |
| modified | `.mcp.json` | — | Update MCP config/env surface. | `git status --short` |
| modified | `Cargo.toml` | — | Add metadata dependencies and bump version to `0.7.0`. | `cargo check` |
| modified | `Cargo.lock` | — | Lock new dependencies and package version. | `cargo check` |
| modified | `README.md` | — | Document container, stats, identify, and auto-retag behavior. | `git diff --stat` |
| created | `docs/container.md` | — | Document container usage and mounted-library workflows. | `git status --short` |
| modified | `docs/musicbrainz-acoustid.md` | — | Update MusicBrainz/AcoustID plan to implemented workflow. | `git diff --stat` |
| modified | `gemini-extension.json` | — | Expose new settings and keep version in sync. | `git status --short` |
| modified | `scripts/check-packaging.sh` | — | Allow the new `FPCALC_PATH` environment variable. | `git status --short` |
| modified | `skills/ytdl/SKILL.md` | — | Document new metadata and container behavior for agents. | `git status --short` |
| modified | `src/bootstrap_tests.rs` | — | Adjust packaging/bootstrap expectations. | `git status --short` |
| modified | `src/config.rs` | — | Add metadata, history, Plex, and fpcalc config fields. | `git status --short` |
| modified | `src/config_tests.rs` | — | Cover new environment configuration. | `cargo test` |
| created | `src/identify.rs` | — | Implement fpcalc, AcoustID, MusicBrainz, and identify rendering. | `cargo test` |
| created | `src/identify/musicbrainz.rs` | — | Implement MusicBrainz lookup and retag previews. | `cargo test` |
| created | `src/identify/musicbrainz_tests.rs` | — | Test MusicBrainz response parsing. | `cargo test` |
| created | `src/identify/tagger.rs` | — | Write canonical tags with Lofty. | `cargo test` |
| created | `src/identify/tagger_tests.rs` | — | Test tag writes and generated FLAC persistence. | `cargo test` |
| created | `src/identify_tests.rs` | — | Test identify orchestration and fpcalc warning handling. | `cargo test` |
| modified | `src/main.rs` | — | Wire new modules. | `git status --short` |
| modified | `src/mcp.rs` | — | Expose identify, stats, and search UI tooling. | `mcporter list` |
| modified | `src/model.rs` | — | Add tool inputs and metadata options. | `cargo test` |
| modified | `src/model_tests.rs` | — | Cover new input models. | `cargo test` |
| modified | `src/plex_tests.rs` | — | Adjust Plex playlist coverage. | `cargo test` |
| modified | `src/service.rs` | — | Add stats, identify, and automatic retag orchestration. | `cargo test` |
| modified | `src/service/format.rs` | — | Render metadata retag summaries. | `cargo test` |
| created | `src/service/render_tests.rs` | — | Cover markdown/JSON rendering. | `cargo test` |
| created | `src/service/stats_identify_tests.rs` | — | Cover stats and identify service paths. | `cargo test` |
| modified | `src/service_tests.rs` | — | Cover download, search, and auto-retag behavior. | `cargo test` |

## Beads Activity

No bead activity observed during this quick-push wrap-up. The session work was
tracked through git branch state and direct verification commands.

## Repository Maintenance

### Plans

No plan files were moved during quick-push. The quick-push workflow kept plan
and stale-doc maintenance out of scope except for docs directly updated by this
feature.

### Beads

No beads were created, edited, or closed during this session.

### Worktrees and branches

`git worktree list --porcelain` showed one worktree:
`/home/jmagar/workspace/ytdl-rmcp` on `codex/metadata-playlist-sync`. No branch
or worktree cleanup was performed.

### Stale docs

`README.md`, `docs/container.md`, and `docs/musicbrainz-acoustid.md` were updated
because the implementation now supports a deployed container and automatic
download retagging.

## Tools and Skills Used

- **Skills.** `vibin:repo-status`, `vibin:quick-push`,
  `testing:mcporter`, `arrs:plex`, and `superpowers:test-driven-development`.
- **Shell commands.** Git status/diff, Cargo verification, Docker build/load,
  SSH commands on tootie, Plex helper calls, and `mcporter` MCP smoke tests.
- **MCP tooling.** `mcporter` listed and called the deployed stdio server through
  the tootie wrapper.
- **Container tooling.** Docker built the local image, transferred it to tootie,
  and recreated the Compose service.
- **Plex tooling.** The Plex helper listed libraries, checked the playlist, and
  triggered a refresh of section `9`.

## Commands Executed

| command | result |
| --- | --- |
| `docker build -t ytdl-rmcp:local .` | Built the local container image. |
| `docker save ytdl-rmcp:local \| gzip \| ssh tootie 'gunzip \| docker load'` | Loaded rebuilt images on tootie. |
| `ssh tootie 'cd /mnt/cache/compose/ytdl-rmcp && docker compose up -d --force-recreate'` | Recreated the deployed container. |
| `mcporter list --stdio ssh --stdio-arg tootie --stdio-arg /mnt/cache/compose/ytdl-rmcp/mcp-stdio.sh --schema --json` | MCP schema listed successfully. |
| `mcporter call --stdio ssh --stdio-arg tootie --stdio-arg /mnt/cache/compose/ytdl-rmcp/mcp-stdio.sh youtube_stats limit:1 response_format:json` | Deployed MCP call succeeded. |
| `cargo fmt --all --check` | Passed. |
| `cargo test` | Passed, 75 tests. |
| `cargo clippy --all-targets -- -D warnings` | Passed. |
| `cargo check` | Passed and confirmed `ytdl-rmcp v0.7.0`. |

## Errors Encountered

- A direct path sample for a file with apostrophes failed due to shell quoting;
  rerun with environment-injected paths fixed the probe.
- A long 28-file metadata write through `mcporter` hit the 300-second client
  timeout; rerunning per file completed all 28 writes successfully.
- A shell loop used `path` as a zsh variable name, which mutated `PATH`; rerun
  under bash with `audio_path` fixed the loop.
- `fpcalc` returned nonzero on some MP3s while still emitting valid fingerprints;
  code now parses valid stdout before treating the status as fatal.

## Behavior Changes (Before/After)

| area | before | after |
| --- | --- | --- |
| Container deployment | No Compose stack for ytdl-rmcp on tootie. | Compose stack runs at `/mnt/cache/compose/ytdl-rmcp`. |
| Existing metadata | yt-dlp library relied mostly on YouTube-derived tags. | 103 of 118 files have MusicBrainz/AcoustID-backed tags. |
| Future downloads | MusicBrainz retagging required an explicit `youtube_identify` call. | `youtube_download` auto-retags downloaded audio before transfer when AcoustID is configured. |
| fpcalc warnings | Nonzero `fpcalc` status blocked otherwise valid fingerprints. | Valid `DURATION` and `FINGERPRINT` stdout is accepted. |
| Distribution | Container image was local-only. | CI and GHCR publish workflow are in the repo. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo fmt --all --check` | Formatting clean. | Passed. | pass |
| `cargo test` | All tests pass. | 75 passed. | pass |
| `cargo clippy --all-targets -- -D warnings` | No clippy warnings. | Passed. | pass |
| `cargo check` | Version bump compiles. | Checked `ytdl-rmcp v0.7.0`. | pass |
| `docker run --rm ytdl-rmcp:local --version` | Container binary starts. | Verified during container work. | pass |
| `mcporter list ... --schema --json` | Deployed MCP server responds. | Status `ok`, 6 tools. | pass |
| Plex playlist count | Playlist should match library count. | `yt-dlp Downloads` and filesystem both reported 118. | pass |

## Risks and Rollback

- MusicBrainz/AcoustID matching is network-dependent and rate-limited. Roll back
  by unsetting `YTDLP_ACOUSTID_CLIENT_KEY` or reverting the auto-retag commit.
- Tag writes are best-effort, but they do mutate audio files. Existing files
  already retagged on tootie would need restore from backups or re-downloads for
  a full metadata rollback.
- The deployed Compose stack currently uses the locally loaded `ytdl-rmcp:local`
  image until the GHCR workflow publishes from `main`.

## Decisions Not Taken

- Did not force MusicBrainz metadata when confidence was low; 15 existing files
  were left unchanged.
- Did not run broad branch cleanup during quick-push because the user requested
  a publish workflow, not repository pruning.
- Did not force-push; the branch tracks `origin/codex/metadata-playlist-sync`.

## References

- MusicBrainz API: https://musicbrainz.org/doc/MusicBrainz_API
- AcoustID web service: https://acoustid.org/webservice
- Deployed Compose path: `/mnt/cache/compose/ytdl-rmcp`

## Open Questions

- Whether to switch tootie Compose from `ytdl-rmcp:local` to
  `ghcr.io/jmagar/ytdl-rmcp:main` after the main-branch workflow publishes.

## Next Steps

1. Commit and push the feature changes after this session artifact is saved.
2. After merge to `main`, confirm the GHCR image publishes and update tootie
   Compose to use the published image if desired.
3. Download a fresh track through `youtube_download` and confirm the response
   includes `metadata_retag` with the expected counts.
