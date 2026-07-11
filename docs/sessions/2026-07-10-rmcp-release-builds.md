---
date: 2026-07-10 23:49:49 EST
repo: git@github.com:jmagar/ytdl-rmcp.git
branch: main
head: 808953a
working directory: /home/jmagar/workspace/ytdl-mcp
worktree: /home/jmagar/workspace/ytdl-mcp
---

# RMCP release and build recovery session

## User Request

Jacob asked to finish npm setup and release work across the Rust MCP server family, make npm package READMEs match repo READMEs, verify package usage through `npx`, merge the work into `main`, cut releases, investigate build failures, and then save this session to markdown.

## Session Overview

The session completed npm/readme packaging fixes, landed release and CI repairs across the RMCP repos, verified current `main` status with GitHub Actions, and documented the remaining follow-ups. The final save operation produced this path-limited session artifact in `ytdl-rmcp`.

## Sequence of Events

1. Renamed and standardized the Rust MCP package surfaces around the `<service>-rmcp` repo/package pattern and `r<service>` CLI aliases where applicable.
2. Updated npm launchers and README packaging so npm pages use the same README content as the corresponding repo.
3. Published and verified npm launcher behavior for key packages, including `ytdl-rmcp@0.7.4` through `npx -y ytdl-rmcp@0.7.4 --version`.
4. Fixed build and release failures in Synapse, Cortex, Apprise, UniFi, Unraid, and ytdl-related packaging paths.
5. Rechecked live GitHub Actions state for the RMCP repos and separated current `main` failures from historical tag failures, superseded release-please branch failures, cancelled runs, and dependabot-only failures.
6. Ran the `vibin:save-to-md` workflow, performed the required repository maintenance pass, and wrote this session note.

## Key Findings

- `ytdl-rmcp` current `main` is `808953a` and matches `origin/main`; release flow was green, while a later `OpenWiki Update` workflow on `main` was failing.
- `apprise-rmcp` current `main` is `f725433`; CI, Docker Publish, Sync marketplace-no-mcp, and release-please were all green after the fixes.
- `arcane-rmcp` current `main` had new failures on `CI` and `CodeQL` for `b432b57 chore: move arcane mcp to 40110`; this appeared after earlier release work and remains a follow-up.
- `unifi-rmcp` had moved forward to `84fbe6f` after release-please merge; latest main `CI` and Docker were still in progress during the save pass.
- `cortex` local checkout was on `main` at `d7cd583` but had unrelated unstaged deletions under `plugins/cortex/skills/redeploy/`.
- Lumen semantic search was requested first for code discovery, but `mcp__lumen__semantic_search` returned `ensure fresh: embed batch: all embedding servers are unhealthy`, so direct command evidence was used.

## Technical Decisions

- Release-tag workflow failures caused by optional decoration steps were hardened instead of blocking the build: Apprise stopped asking the SBOM action to attach release assets, and UniFi skips MCP Registry publish when `MCP_PRIVATE_KEY` is absent.
- Apprise kept a narrow `cargo audit` ignore for `RUSTSEC-2023-0071` because the RSA advisory is inherited through `lab-auth`/JWT dependencies and no fixed RSA release was available.
- Unraid test/docs crate references were updated from `unraid_mcp` to `unraid_rmcp` while preserving the runtime session cookie name.
- Superseded or historical failures were not treated as current blockers when a newer run on the same branch/SHA had succeeded.
- The final session note commit was intentionally path-limited to this generated file because the `ytdl-rmcp` worktree already had unrelated dirty packaging files.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `/home/jmagar/workspace/ytdl-mcp/packages/ytdl-rmcp/lib/platform.js` | - | Align npm launcher asset and binary names with `rytdl`/release assets. | `npx -y ytdl-rmcp@0.7.4 --version` returned `rytdl 0.7.4`. |
| modified | `/home/jmagar/workspace/ytdl-mcp/.github/workflows/*` and Docker/package paths | - | Align release/build workflows with the renamed `rytdl` binary. | `ytdl-rmcp` release/main workflows were green in `gh run list`. |
| modified | `/home/jmagar/workspace/synapse/tests/plugin_contract.rs` | - | Update plugin setup contract for the renamed binary helper. | `cargo test --test plugin_contract` passed. |
| modified | `/home/jmagar/workspace/synapse/.github/workflows/codeql.yml` | - | Align CodeQL init/analyze versions after dependabot drift. | CodeQL main run succeeded on `b5e6085`. |
| modified | `/home/jmagar/workspace/synapse/Cargo.lock` | - | Replace yanked `crypto-bigint 0.7.3` with `0.7.5`. | `cargo deny check` passed locally. |
| modified | `/home/jmagar/workspace/cortex/tests/test_live.sh` | - | Stabilize live smoke by ignoring volatile disk metrics and removing stale `ask-history` route expectation. | `CORTEX_TOKEN=ci-integration-token bash tests/test_live.sh` passed locally. |
| modified | `/home/jmagar/workspace/apprise-rmcp/.github/workflows/docker-publish.yml` | - | Prevent SBOM release asset upload from failing tag Docker publish. | Apprise Docker Publish on `main` later succeeded. |
| modified | `/home/jmagar/workspace/apprise-rmcp/src/cli.rs` and logging files | - | Fix Rust 1.97 clippy failures and align setup/doctor contracts. | `cargo clippy -- -D warnings` passed locally and in CI. |
| created | `/home/jmagar/workspace/apprise-rmcp/.cargo/audit.toml` | - | Record narrow audit ignore for inherited RSA advisory with no fixed release. | `cargo audit` passed locally and in CI. |
| modified | `/home/jmagar/workspace/apprise-rmcp/README.md`, docs, package README, tests, hook config | - | Update stale port `8765` to `40050` and restore direct `rapprise setup plugin-hook` hook contract. | `cargo nextest run --profile ci` passed locally and CI succeeded. |
| modified | `/home/jmagar/workspace/unifi-rmcp/.github/workflows/docker-publish.yml` | - | Skip MCP Registry publish when `MCP_PRIVATE_KEY` is not configured. | UniFi main Docker Publish succeeded. |
| modified | `/home/jmagar/workspace/unraid-rmcp/tests/*.rs`, docs, OpenWiki snippets, `examples/mock_unraid.rs` | - | Replace stale crate path `unraid_mcp` with `unraid_rmcp`. | `cargo test --all-targets --features test-support --no-run` passed locally and CI succeeded. |
| created | `/home/jmagar/workspace/ytdl-mcp/docs/sessions/2026-07-10-rmcp-release-builds.md` | - | Save this full session log. | Created by the final `vibin:save-to-md` workflow. |

## Beads Activity

No bead activity observed.

Evidence:
- `bd list --all --sort updated --reverse --limit 100 --json` returned `[]`.
- `.beads/interactions.jsonl` was not present or had no readable recent interactions.

## Repository Maintenance

### Plans

No completed plan files were found under `docs/plans/`; no plan files were moved to `docs/plans/complete/`.

### Beads

No ytdl-rmcp bead activity was observed, and no bead changes were made during the save pass.

### Worktrees and branches

`git worktree list --porcelain` showed:
- Active main worktree: `/home/jmagar/workspace/ytdl-mcp` at `808953a`.
- No-MCP worktree: `/home/jmagar/workspace/_no_mcp_worktrees/ytdl-mcp` on `marketplace-no-mcp` at `93a90a1`.

The no-MCP worktree was left untouched because project memory says `marketplace-no-mcp` is an intentional long-lived marketplace variant, not stale cleanup. Remote dependabot branches and release-please branches were also left untouched because they were either active automation branches or not proven safe to delete.

### Stale docs

Several OpenWiki workflow failures were observed across repos after the build fixes, including `ytdl-rmcp`, `apprise-rmcp`, `gotify-rmcp`, `synapse-rmcp`, `tailscale-rmcp`, `unraid-rmcp`, and `cortex`. These were documented as follow-up work rather than edited during the session save.

### Dirty state

The ytdl-rmcp working tree already had unrelated dirty files before this note was written:

```text
 M .claude-plugin/plugin.json
 M .gitignore
 M .mcp.json
 M README.md
 M gemini-extension.json
 M mcpb/manifest.json
 M scripts/check-packaging.sh
```

They were not staged or committed as part of this session artifact.

## Tools and Skills Used

- **Shell commands.** Used for git state, GitHub Actions state, Cargo checks, npm verification, release verification, and final session commit/push.
- **GitHub CLI.** Used to inspect Actions runs and verify current branch health across the RMCP repos.
- **Cargo tooling.** Used for `cargo test`, `cargo clippy`, `cargo fmt`, `cargo audit`, `cargo deny`, `cargo check`, and targeted compile verification.
- **npm/npx.** Used to verify npm package contents and launcher behavior.
- **Labby/Labby-related setup.** Earlier session work checked Labby env/setup and npm stdio/npx integration assumptions.
- **Lumen semantic search.** Invoked first during the save turn as instructed; it failed because embedding servers were unhealthy, so direct evidence commands were used.
- **Skills.** Used `vibin:save-to-md`; also relied on superpowers workflow discipline and verification-before-completion behavior.
- **Multi-agent/subagents.** The broader session included user-requested parallel-agent style work across repos; final validation was done by the main agent.

## Commands Executed

| command | result |
| --- | --- |
| `git remote get-url origin && git branch --show-current && git rev-parse --short HEAD && git status --short` | Confirmed `ytdl-rmcp` repo, branch `main`, HEAD `808953a`, and existing dirty files. |
| `git worktree list --porcelain && git branch -vv && git branch -r -vv` | Confirmed main and `marketplace-no-mcp` worktrees and remote branches. |
| `gh run list --repo jmagar/<repo> --limit 5 --json ...` | Verified current CI/release/Docker state across ytdl, Apprise, Arcane, Gotify, Synapse, Tailscale, UniFi, Unraid, and Cortex. |
| `cargo test --test plugin_contract` | Passed in Synapse after plugin contract update. |
| `cargo deny check` | Passed in Synapse after lockfile update. |
| `CORTEX_TOKEN=ci-integration-token bash tests/test_live.sh` | Passed in Cortex after live smoke stabilization. |
| `cargo test --all-targets --features test-support --no-run` | Passed in Unraid after crate reference fixes. |
| `cargo fmt --all --check`, `cargo clippy -- -D warnings`, `cargo audit`, `cargo nextest run --profile ci` | Passed in Apprise after CI contract fixes. |
| `npx -y ytdl-rmcp@0.7.4 --version` | Returned `rytdl 0.7.4`. |
| `bd list --all --sort updated --reverse --limit 100 --json` | Returned `[]` in ytdl-rmcp. |

## Errors Encountered

- **Lumen semantic search unavailable.** `mcp__lumen__semantic_search` returned `ensure fresh: embed batch: all embedding servers are unhealthy`; direct command evidence was used instead.
- **Synapse CI failures.** Plugin contract, CodeQL version mismatch, generated OpenAPI/fmt drift, and yanked `crypto-bigint` were resolved by targeted commits.
- **Cortex live smoke failure.** Volatile disk stats and a stale `/api/sessions/ask-history` expectation caused failures; `tests/test_live.sh` was updated.
- **Apprise CI failures.** Rust 1.97 clippy lints, stale test contracts, old default port docs/tests, and patchable audit issues were resolved. The inherited RSA advisory was ignored narrowly with rationale.
- **Apprise release tag Docker failure.** SBOM action tried to attach release assets and hit GitHub integration permission limits; workflow now disables that release-asset upload.
- **UniFi tag Docker failure.** `mcp-publisher login` received an empty `MCP_PRIVATE_KEY`; workflow now skips registry publish with a notice if the secret is absent.
- **Unraid CI failure.** Tests/examples still imported `unraid_mcp`; imports/docs were updated to `unraid_rmcp`.

## Behavior Changes (Before/After)

| area | before | after |
| --- | --- | --- |
| ytdl npm launcher | npm launcher expected stale asset/binary names. | `ytdl-rmcp@0.7.4` launches `rytdl` via `npx`. |
| Apprise CI | Failed fmt, clippy, audit, and setup contract tests. | CI succeeded on `main` at `f725433`. |
| Apprise Docker release path | SBOM release attachment could fail the release build. | SBOM remains an artifact; release asset upload is disabled. |
| UniFi Docker release path | Missing `MCP_PRIVATE_KEY` failed Docker publish. | Registry publish is skipped when the secret is absent. |
| Unraid tests | Tests/examples referred to old crate name `unraid_mcp`. | Tests compile against `unraid_rmcp`. |
| Cortex live smoke | Volatile stats and stale route expectation failed smoke tests. | Smoke script tolerates volatile disk fields and no longer checks the stale route. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `gh run list --repo jmagar/apprise-rmcp --limit 5` | Latest main CI/Docker green. | `CI`, `Docker Publish`, `Sync marketplace-no-mcp`, and `release-please` were green at `f725433`. | pass |
| `gh run list --repo jmagar/synapse-rmcp --limit 5` | Main and release-please branch green. | Main CodeQL/MSRV/release-please green; release branch CI/MSRV green. | pass |
| `gh run list --repo jmagar/unraid-rmcp --limit 5` | Latest main CI/Docker green. | CI, Docker, Sync, and release-please green at `90e9681`. | pass |
| `gh run list --repo jmagar/unifi-rmcp --limit 5` | Main green. | Main Sync green; CI/Docker were in progress at `84fbe6f` during final save sweep. Earlier main at `2647842` was green. | warn |
| `gh run list --repo jmagar/cortex --limit 5` | Latest non-superseded release branch green. | Latest release branch `348b332` CI/Docker green; earlier release branch `06cf4f8` CI failed and was superseded. | pass |
| `gh run list --repo jmagar/arcane-rmcp --limit 5` | No current main failures. | Current main `b432b57` had CI and CodeQL failures. | fail |
| `cargo nextest run --profile ci` in Apprise | Tests pass. | 19 tests passed locally. | pass |
| `cargo audit` in Apprise | No unhandled advisories. | Passed locally after patch bumps and narrow RSA ignore. | pass |
| `cargo test --all-targets --features test-support --no-run` in Unraid | Compile all test targets. | Passed locally. | pass |
| `npx -y ytdl-rmcp@0.7.4 --version` | Launcher runs installed release. | Returned `rytdl 0.7.4`. | pass |

## Risks and Rollback

- Apprise contains a narrow audit ignore for an inherited RSA advisory with no fixed release; rollback is to remove `.cargo/audit.toml` after `lab-auth`/JWT dependencies eliminate the advisory.
- UniFi now skips MCP Registry publish without `MCP_PRIVATE_KEY`; if registry publication is mandatory, configure the secret and rerun the release workflow.
- ytdl-rmcp has unrelated dirty packaging files in the local worktree; this session artifact commit intentionally does not include them.
- The save pass did not fix new Arcane main CI/CodeQL failures or OpenWiki failures; those require follow-up investigation.

## Decisions Not Taken

- Did not delete the `marketplace-no-mcp` worktree or branch because it is documented as an intentional long-lived marketplace variant.
- Did not force-push, reset, or revert any unrelated dirty files.
- Did not locally rebuild every release binary after the final commits; verification relied on targeted local tests/builds and GitHub Actions where configured.
- Did not fix OpenWiki workflow failures during the session save because that would expand the scope beyond documenting the completed release/build recovery.

## References

- GitHub Actions run lists for `jmagar/ytdl-rmcp`, `jmagar/apprise-rmcp`, `jmagar/arcane-rmcp`, `jmagar/gotify-rmcp`, `jmagar/synapse-rmcp`, `jmagar/tailscale-rmcp`, `jmagar/unifi-rmcp`, `jmagar/unraid-rmcp`, and `jmagar/cortex`.
- Local repo state from `/home/jmagar/workspace/ytdl-mcp`, `/home/jmagar/workspace/apprise-rmcp`, `/home/jmagar/workspace/synapse`, `/home/jmagar/workspace/cortex`, `/home/jmagar/workspace/unifi-rmcp`, and `/home/jmagar/workspace/unraid-rmcp`.

## Open Questions

- Why is Arcane current `main` failing CI and CodeQL at `b432b57 chore: move arcane mcp to 40110`?
- Why are OpenWiki Update workflows failing across several repos?
- Should the remaining GitHub-reported Apprise moderate RSA advisory be tracked upstream in `lab-auth` or accepted as a known inherited no-fixed-release advisory for now?
- Did the latest UniFi `84fbe6f` main CI/Docker runs finish green after the save-pass snapshot?

## Next Steps

- Investigate Arcane main CI/CodeQL failure first, because it is a current `main` failure rather than historical or dependabot noise.
- Recheck UniFi latest `main` runs for `84fbe6f` and confirm CI/Docker completion.
- Open a focused follow-up for OpenWiki Update workflow failures across ytdl, Apprise, Gotify, Synapse, Tailscale, Unraid, and Cortex.
- If strict local binary rebuild evidence is required for every RMCP repo, run a dedicated build sweep with `cargo build --release` or each repo's release-equivalent command and record binary paths/checksums.
