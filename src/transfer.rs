//! Transfer staged files to a configured target. SSH targets (`host:/path`)
//! preserve the original rsync/scp behavior, local targets (`/path`) use rsync
//! when available, and rclone targets (`remote:path`) use `rclone copy`.
//!
//! Key-based, non-interactive auth is assumed: `-o BatchMode=yes` plus
//! `-o StrictHostKeyChecking=accept-new` guarantee we fail fast rather than
//! hang on a prompt — a stdio MCP server has no TTY to answer one.

use std::path::Path;

use anyhow::{bail, Result};

use tokio::process::Command;

use crate::util::{command_error, run_capped};

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;

/// Tail-cap applied to transfer subprocess (ssh/rsync/scp) stderr, mirroring the
/// downloader's 16 KiB bound so a misbehaving remote shell can't stream
/// unbounded diagnostics into memory.
const STDERR_CAP: usize = 16 * 1024;

/// Typed failures for the transfer input-validation boundary (`RemoteSpec` /
/// `RemotePath`). Hand-rolled `Error`/`Display` (the crate does not depend on
/// `thiserror`) so tests can assert on a specific variant rather than
/// string-matching, while `parse` still surfaces as `anyhow::Result` to callers
/// (anyhow auto-converts any `std::error::Error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferValidationError {
    /// Value was empty or only whitespace.
    Empty { field: &'static str },
    /// Value started with `-`, so a shell/command could read it as an option.
    LeadingDash { field: &'static str },
    /// Value contained whitespace and/or control characters.
    BadChars { field: &'static str },
    /// Path contained a `..` segment (directory traversal).
    Traversal { field: &'static str },
    /// Path was not absolute (did not start with `/`).
    NotAbsolute { field: &'static str },
}

impl std::fmt::Display for TransferValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty { field } => write!(f, "{field} must not be empty"),
            Self::LeadingDash { field } => write!(f, "{field} must not start with '-'"),
            Self::BadChars { field } => {
                // Shared variant: `RemotePath` rejects only control characters
                // (whitespace is allowed in names like "Title [id]"), while
                // `RemoteSpec` additionally rejects whitespace. Keep the message
                // truthful for both uses rather than over-claiming for the path.
                write!(
                    f,
                    "{field} must not contain control characters (the SSH remote also rejects whitespace)"
                )
            }
            Self::Traversal { field } => {
                write!(f, "{field} must not contain a '..' path segment")
            }
            Self::NotAbsolute { field } => {
                write!(f, "{field} must be an absolute path starting with '/'")
            }
        }
    }
}

impl std::error::Error for TransferValidationError {}

/// Shared leading check for both `RemoteSpec` and `RemotePath`: reject an
/// empty/whitespace-only value. Both validators run this first and in the same
/// position, so consolidating it here keeps the two paths from drifting.
///
/// The leading-dash check is intentionally NOT folded in: `RemoteSpec` rejects a
/// leading dash *before* its bad-chars check while `RemotePath` rejects control
/// characters *before* its leading-dash check, so a single shared ordering would
/// change which variant wins for a value that is both control-bearing and
/// dash-led. Each caller keeps its own leading-dash check to preserve that.
fn reject_empty(
    raw: &str,
    field: &'static str,
) -> std::result::Result<(), TransferValidationError> {
    if raw.trim().is_empty() {
        return Err(TransferValidationError::Empty { field });
    }
    Ok(())
}

fn validate_absolute_path(
    raw: impl Into<String>,
    field: &'static str,
) -> std::result::Result<String, TransferValidationError> {
    let raw = raw.into();
    reject_empty(&raw, field)?;
    if raw.chars().any(char::is_control) {
        return Err(TransferValidationError::BadChars { field });
    }
    if raw.starts_with('-') {
        return Err(TransferValidationError::LeadingDash { field });
    }
    if !raw.starts_with('/') {
        return Err(TransferValidationError::NotAbsolute { field });
    }
    if raw.split('/').any(|segment| segment == "..") {
        return Err(TransferValidationError::Traversal { field });
    }
    Ok(raw)
}

fn validate_local_path(
    raw: impl Into<String>,
    field: &'static str,
) -> std::result::Result<String, TransferValidationError> {
    let raw = raw.into();
    reject_empty(&raw, field)?;
    if raw.chars().any(char::is_control) {
        return Err(TransferValidationError::BadChars { field });
    }
    if raw.starts_with('-') {
        return Err(TransferValidationError::LeadingDash { field });
    }
    if !is_supported_local_absolute_path(&raw) {
        return Err(TransferValidationError::NotAbsolute { field });
    }
    if raw.split(['/', '\\']).any(|segment| segment == "..") {
        return Err(TransferValidationError::Traversal { field });
    }
    Ok(raw)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSpec(String);

impl RemoteSpec {
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        Ok(Self::parse_typed(raw)?)
    }

    fn parse_typed(raw: impl Into<String>) -> std::result::Result<Self, TransferValidationError> {
        const FIELD: &str = "SSH remote";
        let raw = raw.into();
        reject_empty(&raw, FIELD)?;
        if raw.starts_with('-') {
            return Err(TransferValidationError::LeadingDash { field: FIELD });
        }
        if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(TransferValidationError::BadChars { field: FIELD });
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePath(String);

impl RemotePath {
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        Ok(Self::parse_typed(raw)?)
    }

    /// Validate an SSH destination path. The remote layout is `Artist/Title
    /// [id]` under an absolute media root, so we require an absolute path and
    /// reject anything that could redirect writes outside that root or be read
    /// as a command-line option:
    ///   - empty / whitespace-only
    ///   - any control character
    ///   - a leading `-` (option-injection defense, matching `RemoteSpec`)
    ///   - non-absolute paths (must start with `/`)
    ///   - any `..` path segment (directory traversal)
    fn parse_typed(raw: impl Into<String>) -> std::result::Result<Self, TransferValidationError> {
        const FIELD: &str = "remote destination path";
        Ok(Self(validate_absolute_path(raw, FIELD)?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPath(String);

impl LocalPath {
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        Ok(Self::parse_typed(raw)?)
    }

    fn parse_typed(raw: impl Into<String>) -> std::result::Result<Self, TransferValidationError> {
        const FIELD: &str = "local destination path";
        Ok(Self(validate_local_path(raw, FIELD)?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RcloneTarget(String);

impl RcloneTarget {
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        reject_empty(&raw, "rclone target")?;
        if raw.starts_with('-') {
            return Err(TransferValidationError::LeadingDash {
                field: "rclone target",
            }
            .into());
        }
        if raw.chars().any(char::is_control) {
            return Err(TransferValidationError::BadChars {
                field: "rclone target",
            }
            .into());
        }
        let Some((remote, path)) = raw.split_once(':') else {
            bail!("rclone target must be in remote:path form");
        };
        if remote.trim().is_empty() || path.trim().is_empty() {
            bail!("rclone target must include both remote and path");
        }
        if remote.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(TransferValidationError::BadChars {
                field: "rclone target",
            }
            .into());
        }
        if path.split('/').any(|segment| segment == "..") {
            return Err(TransferValidationError::Traversal {
                field: "rclone target",
            }
            .into());
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetPath {
    Local(LocalPath),
    Ssh {
        remote: RemoteSpec,
        path: RemotePath,
    },
    Rclone(RcloneTarget),
}

impl TargetPath {
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        reject_empty(&raw, "target path")?;
        if raw.chars().any(char::is_control) {
            return Err(TransferValidationError::BadChars {
                field: "target path",
            }
            .into());
        }
        if is_windows_absolute_path(&raw) {
            #[cfg(windows)]
            {
                return Ok(Self::Local(LocalPath::parse(raw)?));
            }
            #[cfg(not(windows))]
            {
                bail!("Windows absolute local target paths are only supported on Windows");
            }
        }
        if let Some(rest) = raw.strip_prefix("ssh:") {
            let Some((remote, path)) = rest.split_once(":/") else {
                bail!("ssh target must be in ssh:host:/path form");
            };
            return Ok(Self::Ssh {
                remote: RemoteSpec::parse(remote)?,
                path: RemotePath::parse(format!("/{path}"))?,
            });
        }
        if let Some(rest) = raw.strip_prefix("rclone:") {
            return Ok(Self::Rclone(RcloneTarget::parse(rest)?));
        }
        if let Some((remote, path)) = raw.split_once(":/") {
            return Ok(Self::Ssh {
                remote: RemoteSpec::parse(remote)?,
                path: RemotePath::parse(format!("/{path}"))?,
            });
        }
        if raw.contains(':') {
            return Ok(Self::Rclone(RcloneTarget::parse(raw)?));
        }
        Ok(Self::Local(LocalPath::parse(raw)?))
    }

    pub fn display(&self) -> String {
        match self {
            Self::Local(path) => path.as_str().to_string(),
            Self::Ssh { remote, path } => format!("{}:{}", remote.as_str(), path.as_str()),
            Self::Rclone(target) => {
                if target.as_str().contains(":/") {
                    format!("rclone:{}", target.as_str())
                } else {
                    target.as_str().to_string()
                }
            }
        }
    }
}

fn is_windows_absolute_path(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if raw.starts_with("\\\\") {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

fn is_supported_local_absolute_path(raw: &str) -> bool {
    #[cfg(windows)]
    {
        Path::new(raw).is_absolute()
    }
    #[cfg(not(windows))]
    {
        raw.starts_with('/')
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferTarget {
    audio_target: TargetPath,
    video_target: TargetPath,
}

impl TransferTarget {
    pub fn parse_targets(audio_target: &str, video_target: Option<&str>) -> Result<Self> {
        let audio_target = TargetPath::parse(audio_target)?;
        let video_target = match video_target {
            Some(path) => TargetPath::parse(path)?,
            None => audio_target.clone(),
        };
        Ok(Self {
            audio_target,
            video_target,
        })
    }

    pub fn audio_target(&self) -> &TargetPath {
        &self.audio_target
    }

    pub fn video_target(&self) -> &TargetPath {
        &self.video_target
    }

    pub fn contains_local(&self) -> bool {
        matches!(self.audio_target, TargetPath::Local(_))
            || matches!(self.video_target, TargetPath::Local(_))
    }
}

/// Ensure the destination directory tree exists on the remote (idempotent).
#[cfg_attr(windows, allow(dead_code))]
pub async fn ensure_remote_dir(
    remote: &RemoteSpec,
    dest_path: &RemotePath,
    ssh_opts: &[String],
) -> Result<()> {
    let command = remote_mkdir_command(dest_path);
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_opts).arg(remote.as_str()).arg(&command);
    let out = run_capped(&mut cmd, None, Some(STDERR_CAP)).await?;
    if !out.status.success() {
        let detail = command_error((out.stderr.as_str(), out.stdout.as_slice()));
        let remote = remote.as_str();
        let dest_path = dest_path.as_str();
        bail!(
            "could not create '{dest_path}' on '{remote}': {detail}. \
             Check the remote alias, your SSH key, and write permissions."
        );
    }
    Ok(())
}

/// Ensure the destination exists when the target type has a directory primitive.
#[cfg_attr(windows, allow(dead_code))]
pub async fn ensure_target_dir(target: &TargetPath, ssh_opts: &[String]) -> Result<()> {
    match target {
        TargetPath::Local(path) => {
            tokio::fs::create_dir_all(path.as_str()).await?;
            Ok(())
        }
        TargetPath::Ssh { remote, path } => ensure_remote_dir(remote, path, ssh_opts).await,
        TargetPath::Rclone(_) => Ok(()),
    }
}

/// Sync the *contents* of `staging_kind_dir` (an `audio/` or `video/` subdir,
/// including its artist folders) into the parsed target.
pub async fn transfer_to_target(
    staging_kind_dir: &Path,
    target: &TargetPath,
    ssh_opts: &[String],
) -> Result<()> {
    match target {
        TargetPath::Local(path) => transfer_local(staging_kind_dir, path).await,
        TargetPath::Ssh { remote, path } => {
            transfer(staging_kind_dir, remote, path, ssh_opts).await
        }
        TargetPath::Rclone(target) => rclone_copy(staging_kind_dir, target).await,
    }
}

/// Sync the *contents* of `staging_kind_dir` (an `audio/` or `video/` subdir,
/// including its artist folders) into `dest_path` on `remote`.
pub async fn transfer(
    staging_kind_dir: &Path,
    remote: &RemoteSpec,
    dest_path: &RemotePath,
    ssh_opts: &[String],
) -> Result<()> {
    if which::which("rsync").is_ok() {
        rsync(staging_kind_dir, remote, dest_path, ssh_opts).await
    } else {
        scp(staging_kind_dir, remote, dest_path, ssh_opts).await
    }
}

#[cfg(windows)]
pub async fn transfer_file_paths(
    files: &[std::path::PathBuf],
    staging_kind_dir: &Path,
    target: &TargetPath,
    ssh_opts: &[String],
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    transfer_to_target(staging_kind_dir, target, ssh_opts).await
}

async fn transfer_local(dir: &Path, dest_path: &LocalPath) -> Result<()> {
    if which::which("rsync").is_ok() {
        local_rsync(dir, dest_path).await
    } else {
        local_copy(dir, dest_path).await
    }
}

async fn local_rsync(dir: &Path, dest_path: &LocalPath) -> Result<()> {
    let src = format!("{}/", dir.display());
    let target = format!("{}/", dest_path.as_str().trim_end_matches('/'));
    let mut cmd = Command::new("rsync");
    cmd.args(["-a", "--partial", "--human-readable"])
        .arg(&src)
        .arg(&target);
    let out = run_capped(&mut cmd, None, Some(STDERR_CAP)).await?;
    if !out.status.success() {
        bail!(
            "local rsync failed (exit {:?}): {}",
            out.status.code(),
            command_error((out.stderr.as_str(), out.stdout.as_slice()))
        );
    }
    Ok(())
}

async fn local_copy(dir: &Path, dest_path: &LocalPath) -> Result<()> {
    copy_dir_contents(dir, Path::new(dest_path.as_str())).await
}

async fn copy_dir_contents(src: &Path, dest: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dest).await?;
    let src_root = tokio::fs::canonicalize(src).await?;
    let dest_root = tokio::fs::canonicalize(dest).await?;
    if dest_root.starts_with(&src_root) {
        bail!(
            "local destination {} must not be inside source {}",
            dest_root.display(),
            src_root.display()
        );
    }

    let mut stack = vec![(src_root, dest_root)];
    while let Some((current_src, current_dest)) = stack.pop() {
        tokio::fs::create_dir_all(&current_dest).await?;
        let mut entries = tokio::fs::read_dir(&current_src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            let source_path = entry.path();
            let dest_path = current_dest.join(entry.file_name());
            if file_type.is_symlink() {
                bail!(
                    "local copy refuses to follow symlink {}",
                    source_path.display()
                );
            } else if file_type.is_dir() {
                stack.push((source_path, dest_path));
            } else if file_type.is_file() {
                tokio::fs::copy(&source_path, &dest_path).await?;
            }
        }
    }
    Ok(())
}

async fn rclone_copy(dir: &Path, target: &RcloneTarget) -> Result<()> {
    rclone_copy_with_command(dir, target, "rclone").await
}

async fn rclone_copy_with_command(dir: &Path, target: &RcloneTarget, command: &str) -> Result<()> {
    let mut cmd = Command::new(command);
    cmd.args(rclone_copy_args(dir, target));
    let out = run_capped(&mut cmd, None, Some(STDERR_CAP)).await?;
    if !out.status.success() {
        bail!(
            "rclone copy failed (exit {:?}): {}",
            out.status.code(),
            command_error((out.stderr.as_str(), out.stdout.as_slice()))
        );
    }
    Ok(())
}

fn rclone_copy_args(dir: &Path, target: &RcloneTarget) -> Vec<std::ffi::OsString> {
    vec![
        "copy".into(),
        dir.as_os_str().to_os_string(),
        target.as_str().into(),
        "--create-empty-src-dirs".into(),
    ]
}

async fn rsync(
    dir: &Path,
    remote: &RemoteSpec,
    dest_path: &RemotePath,
    ssh_opts: &[String],
) -> Result<()> {
    // Trailing slash on the source copies the contents, not the dir itself.
    let src = format!("{}/", dir.display());
    // `-s`/`--protect-args` sends the path to the remote rsync directly,
    // bypassing remote-shell word-splitting — so spaces are safe and the path
    // must NOT be shell-quoted (quoting would make it parse as relative).
    let target = format!("{}:{}/", remote.as_str(), dest_path.as_str());
    let ssh_cmd = rsync_remote_shell_command(ssh_opts);
    let mut cmd = Command::new("rsync");
    cmd.args(["-a", "--partial", "--human-readable", "-s", "-e", &ssh_cmd])
        .arg(&src)
        .arg(&target);
    let out = run_capped(&mut cmd, None, Some(STDERR_CAP)).await?;
    if !out.status.success() {
        bail!(
            "rsync failed (exit {:?}): {}",
            out.status.code(),
            command_error((out.stderr.as_str(), out.stdout.as_slice()))
        );
    }
    Ok(())
}

async fn scp(
    dir: &Path,
    remote: &RemoteSpec,
    dest_path: &RemotePath,
    ssh_opts: &[String],
) -> Result<()> {
    // scp has no "contents of dir" mode like rsync's trailing slash, so copy
    // each top-level entry (artist folders) recursively into the dest.
    let mut entries = Vec::new();
    for e in std::fs::read_dir(dir)? {
        entries.push(e?.path());
    }
    if entries.is_empty() {
        return Ok(());
    }
    let target = format!("{}:{}/", remote.as_str(), shell_quote(dest_path.as_str()));
    let mut cmd = Command::new("scp");
    cmd.arg("-r").args(ssh_opts);
    for e in &entries {
        cmd.arg(e);
    }
    cmd.arg(&target);
    let out = run_capped(&mut cmd, None, Some(STDERR_CAP)).await?;
    if !out.status.success() {
        bail!(
            "scp failed (exit {:?}): {}",
            out.status.code(),
            command_error((out.stderr.as_str(), out.stdout.as_slice()))
        );
    }
    Ok(())
}

/// Minimal single-quote shell escaping for the remote path (survives spaces).
#[cfg_attr(windows, allow(dead_code))]
fn remote_mkdir_command(dest_path: &RemotePath) -> String {
    format!("mkdir -p -- {}", shell_quote(dest_path.as_str()))
}

fn rsync_remote_shell_command(ssh_opts: &[String]) -> String {
    std::iter::once("ssh".to_string())
        .chain(ssh_opts.iter().map(|arg| shell_quote_if_needed(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn shell_quote_if_needed(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "@%_+=:,./-".contains(c))
    {
        s.to_string()
    } else {
        shell_quote(s)
    }
}
