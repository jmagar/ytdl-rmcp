# Phase 2: Security and Performance

## Phase 1 Context Read

Phase 1 found no new current code-quality or architecture issues and confirmed the earlier transfer-boundary and partial-result findings are remediated.

## Findings

No new Security or Performance findings were identified in the current checkout.

## Verified Prior Remediations

- Resolved - `src/transfer.rs:24`
  `RemoteSpec::parse` rejects empty remotes, option-like remotes beginning with `-`, whitespace, and control characters.
  Impact: tool callers cannot turn the remote parameter into additional SSH-family transport options through obvious argv-boundary tricks.

- Resolved - `src/transfer.rs:47`
  `RemotePath::parse` rejects empty and control-character paths, and all destinations flow through `TransferTarget::parse`.
  Impact: malformed destination paths are rejected before subprocess execution.

- Resolved - `src/transfer.rs:99`
  Remote directory creation uses `remote_mkdir_command`, which shell-quotes the destination path before sending it through the remote user's shell.
  Impact: spaces, single quotes, semicolons, command substitutions, and leading dashes are handled by the quoting layer rather than interpreted as shell syntax.

- Resolved - `src/transfer.rs:202`
  The rsync remote-shell command quotes each SSH option when needed.
  Impact: configured SSH options with spaces survive rsync's `-e` command-string boundary.

- Resolved - `src/downloader.rs:352`
  yt-dlp commands use `kill_on_drop(true)`, configurable timeouts, explicit child kill on timeout, and bounded stderr tail collection.
  Impact: stuck media extraction is bounded and large stderr output is not buffered without limit.

- Resolved - `src/service.rs:122`
  Transfer phases are wrapped in `tokio::time::timeout(cfg.transfer_timeout(), transfer)` and failed transfers keep staging for retry.
  Impact: stuck SSH/rsync/scp phases are bounded at the service orchestration layer.

- Resolved - `src/bootstrap.rs:73`, `src/bootstrap/ytdlp.rs:161`, and `src/bootstrap/ffmpeg.rs:231`
  Optional SHA-256 pins are enforced for override, PATH, cached, and downloaded yt-dlp/ffmpeg executables.
  Impact: locked-down users can require known executable bytes instead of trusting moving upstream assets.

- Resolved - `scripts/fetch-binary.sh:303`
  Claude plugin release downloads require published checksums by default and only allow missing checksums when `YTDL_RMCP_ALLOW_MISSING_CHECKSUM=1`.
  Impact: the plugin install path fails closed for current releases.

## Residual Risks

- Runtime bootstrap still allows unpinned yt-dlp and ffmpeg downloads by default. This is documented and configurable with `YTDLP_SHA256`, `FFMPEG_SHA256`, `YTDLP_PATH`, and `FFMPEG_PATH`, so it remains an accepted trust-model choice rather than a new finding.
- Downloads are still processed sequentially by URL. This favors predictable resource use and simpler archive semantics; no performance issue was found that requires changing it.

## Verification

- `cargo fmt --all --check` - passed.
- `cargo test --all` - passed; 38 tests passed.
- `cargo clippy --all-targets -- -D warnings` - passed.
- `scripts/check-packaging.sh` - passed.
- `bash -n scripts/*.sh` plus JSON syntax checks for plugin/MCP/Gemini/hooks manifests - passed.
- `cargo tree -i aws-lc-sys` - returned exit code 101 with "package ID specification `aws-lc-sys` did not match any packages"; this confirms the documented `aws-lc-sys` cross-compile risk is not present.

## Critical Issues for Phase 3 Context

- None from the current Phase 2 pass.
