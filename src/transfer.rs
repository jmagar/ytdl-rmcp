//! Transfer staged files to an SSH remote. Prefers `rsync`; falls back to `scp`
//! (which ships with OpenSSH on Linux and Windows, where rsync is absent).
//!
//! Key-based, non-interactive auth is assumed: `-o BatchMode=yes` plus
//! `-o StrictHostKeyChecking=accept-new` guarantee we fail fast rather than
//! hang on a prompt — a stdio MCP server has no TTY to answer one.

use std::path::Path;

use anyhow::{bail, Result};
use tokio::process::Command;

use crate::util::command_error;

/// Ensure the destination directory tree exists on the remote (idempotent).
pub async fn ensure_remote_dir(remote: &str, dest_path: &str, ssh_opts: &[String]) -> Result<()> {
    let out = Command::new("ssh")
        .args(ssh_opts)
        .arg(remote)
        .args(["mkdir", "-p", "--", dest_path])
        .output()
        .await?;
    if !out.status.success() {
        let detail = command_error(&out);
        bail!(
            "could not create '{dest_path}' on '{remote}': {detail}. \
             Check the remote alias, your SSH key, and write permissions."
        );
    }
    Ok(())
}

/// Sync the *contents* of `staging_kind_dir` (an `audio/` or `video/` subdir,
/// including its artist folders) into `dest_path` on `remote`.
pub async fn transfer(
    staging_kind_dir: &Path,
    remote: &str,
    dest_path: &str,
    ssh_opts: &[String],
) -> Result<()> {
    if which::which("rsync").is_ok() {
        rsync(staging_kind_dir, remote, dest_path, ssh_opts).await
    } else {
        scp(staging_kind_dir, remote, dest_path, ssh_opts).await
    }
}

async fn rsync(dir: &Path, remote: &str, dest_path: &str, ssh_opts: &[String]) -> Result<()> {
    // Trailing slash on the source copies the contents, not the dir itself.
    let src = format!("{}/", dir.display());
    // `-s`/`--protect-args` sends the path to the remote rsync directly,
    // bypassing remote-shell word-splitting — so spaces are safe and the path
    // must NOT be shell-quoted (quoting would make it parse as relative).
    let target = format!("{remote}:{dest_path}/");
    let ssh_cmd = format!("ssh {}", ssh_opts.join(" "));
    let out = Command::new("rsync")
        .args(["-av", "--partial", "--human-readable", "-s", "-e", &ssh_cmd])
        .arg(&src)
        .arg(&target)
        .output()
        .await?;
    if !out.status.success() {
        bail!(
            "rsync failed (exit {:?}): {}",
            out.status.code(),
            command_error(&out)
        );
    }
    Ok(())
}

async fn scp(dir: &Path, remote: &str, dest_path: &str, ssh_opts: &[String]) -> Result<()> {
    // scp has no "contents of dir" mode like rsync's trailing slash, so copy
    // each top-level entry (artist folders) recursively into the dest.
    let mut entries = Vec::new();
    for e in std::fs::read_dir(dir)? {
        entries.push(e?.path());
    }
    if entries.is_empty() {
        return Ok(());
    }
    let target = format!("{remote}:{}/", shell_quote(dest_path));
    let mut cmd = Command::new("scp");
    cmd.arg("-r").args(ssh_opts);
    for e in &entries {
        cmd.arg(e);
    }
    cmd.arg(&target);
    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "scp failed (exit {:?}): {}",
            out.status.code(),
            command_error(&out)
        );
    }
    Ok(())
}

/// Minimal single-quote shell escaping for the remote path (survives spaces).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
