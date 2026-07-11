# 2026-07-11 - target-path transfer unification

## Scope

- Replaced the split `YTDLP_REMOTE` / `YTDLP_REMOTE_PATH` configuration model with `YTDLP_TARGET_PATH`.
- Added target parsing for local paths, SSH paths, and rclone targets.
- Preserved runtime compatibility for legacy env vars and tool-call fields while moving docs and packaging to the new target model.
- Added `YTDLP_ALLOW_LOCAL_TARGETS` to guard per-call local destinations.

## Review Follow-up

- Addressed Lavra review findings for legacy compatibility, history/response transition fields, cross-platform local copies, rclone ambiguity, rclone traversal validation, bounded transfer output, and packaging drift.
- No beads database was present in this checkout, so no bead could be created for this task.

## Verification

```bash
cargo fmt --all --check
scripts/check-packaging.sh
cargo clippy --all-targets -- -D warnings
cargo test
```

Result: all checks passed; `cargo test` reported 148 passing tests.
