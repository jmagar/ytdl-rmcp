#[cfg(test)]
#[path = "transfer_queue_tests.rs"]
mod tests;

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Config;

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct TransferFailureManifestInput {
    pub staging_path: PathBuf,
    pub targets: Vec<(String, String)>,
    pub files: Vec<PathBuf>,
    pub last_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TransferQueueEntry {
    pub version: u32,
    pub manifest_id: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub staging_path: String,
    pub targets: Vec<TransferQueueTarget>,
    pub files: Vec<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    #[serde(skip)]
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TransferQueueTarget {
    pub kind: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TransferQueueList {
    pub queue_dir: String,
    pub entries: Vec<TransferQueueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TransferQueuePruneResult {
    pub pruned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub(crate) struct TransferQueueDrainResult {
    pub retried: usize,
    pub completed: usize,
    pub failed: usize,
    pub entries: Vec<TransferQueueEntry>,
    pub errors: Vec<String>,
}

pub(crate) fn record_failed_transfer(
    cfg: &Config,
    input: TransferFailureManifestInput,
) -> Result<TransferQueueEntry> {
    let queue_dir = queue_dir(cfg);
    fs::create_dir_all(&queue_dir)
        .with_context(|| format!("create transfer queue directory {}", queue_dir.display()))?;
    let _lock = QueueLock::acquire(&queue_dir)?;
    let now = timestamp_now();
    let manifest_id = manifest_id(&input.staging_path, &now);
    let manifest_path = queue_dir.join(format!("{manifest_id}.json"));
    let entry = TransferQueueEntry {
        version: MANIFEST_VERSION,
        manifest_id,
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
        staging_path: input.staging_path.display().to_string(),
        targets: input
            .targets
            .into_iter()
            .map(|(kind, target_path)| TransferQueueTarget { kind, target_path })
            .collect(),
        files: input
            .files
            .into_iter()
            .map(|path| path.display().to_string())
            .collect(),
        attempts: 0,
        last_error: Some(redact_transfer_error(&input.last_error)),
        manifest_path,
    };
    write_manifest(&entry)?;
    Ok(entry)
}

pub(crate) fn list_queue(cfg: &Config) -> Result<TransferQueueList> {
    let queue_dir = queue_dir(cfg);
    let _lock = QueueLock::acquire(&queue_dir)?;
    let entries = read_entries_unlocked(&queue_dir)?;
    Ok(TransferQueueList {
        queue_dir: queue_dir.display().to_string(),
        entries,
    })
}

pub(crate) fn prune_missing(cfg: &Config) -> Result<TransferQueuePruneResult> {
    let queue_dir = queue_dir(cfg);
    let _lock = QueueLock::acquire(&queue_dir)?;
    let mut pruned = 0;
    if queue_dir.is_dir() {
        for dir_entry in fs::read_dir(&queue_dir)? {
            let path = dir_entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let entry = read_manifest(&path)?;
            if !Path::new(&entry.staging_path).is_dir() {
                fs::remove_file(&path)?;
                pruned += 1;
            }
        }
    }
    Ok(TransferQueuePruneResult { pruned })
}

pub(crate) async fn retry_one(
    cfg: &Config,
    manifest_id: &str,
    keep_local: bool,
) -> Result<TransferQueueDrainResult> {
    let mut result = TransferQueueDrainResult {
        retried: 1,
        completed: 0,
        failed: 0,
        entries: Vec::new(),
        errors: Vec::new(),
    };
    record_retry_outcome(&mut result, retry_entry(cfg, manifest_id, keep_local).await);
    Ok(result)
}

pub(crate) async fn retry_all(cfg: &Config, keep_local: bool) -> Result<TransferQueueDrainResult> {
    let queue_dir = queue_dir(cfg);
    let ids: Vec<String> = {
        let _lock = QueueLock::acquire(&queue_dir)?;
        read_entries_unlocked(&queue_dir)?
            .into_iter()
            .map(|entry| entry.manifest_id)
            .collect()
    };
    let mut result = TransferQueueDrainResult {
        retried: 0,
        completed: 0,
        failed: 0,
        entries: Vec::new(),
        errors: Vec::new(),
    };
    for manifest_id in ids {
        result.retried += 1;
        record_retry_outcome(
            &mut result,
            retry_entry(cfg, &manifest_id, keep_local).await,
        );
    }
    Ok(result)
}

fn record_retry_outcome(
    result: &mut TransferQueueDrainResult,
    outcome: Result<TransferQueueEntry>,
) {
    match outcome {
        Ok(entry) if entry.status == "completed" => {
            result.completed += 1;
            result.entries.push(entry);
        }
        Ok(entry) => {
            result.failed += 1;
            result.errors.extend(entry.last_error.iter().cloned());
            result.entries.push(entry);
        }
        Err(error) => {
            result.failed += 1;
            result
                .errors
                .push(redact_transfer_error(&error.to_string()));
        }
    }
}

pub(crate) fn queue_dir(cfg: &Config) -> PathBuf {
    let base = cfg
        .history_path
        .as_ref()
        .map(PathBuf::from)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| {
            crate::bootstrap::project_dirs().map(|dirs| {
                dirs.state_dir()
                    .unwrap_or_else(|| dirs.data_dir())
                    .to_path_buf()
            })
        })
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-rmcp-state"));
    base.join("transfer-queue")
}

pub(crate) fn redact_transfer_error(error: &str) -> String {
    let parts = split_whitespace_preserving(error);
    let mut output = String::with_capacity(error.len());
    let mut redact_next = false;
    for (is_whitespace, part) in parts {
        if is_whitespace {
            output.push_str(&part);
            continue;
        }
        if redact_next {
            output.push_str("REDACTED");
            redact_next = false;
            continue;
        }
        let redacted = redact_error_token(&part);
        if part.eq_ignore_ascii_case("bearer") {
            redact_next = true;
        }
        output.push_str(&redacted);
    }
    output
}

fn split_whitespace_preserving(value: &str) -> Vec<(bool, String)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_is_whitespace = None;
    for ch in value.chars() {
        let is_whitespace = ch.is_whitespace();
        if current_is_whitespace == Some(is_whitespace) || current_is_whitespace.is_none() {
            current.push(ch);
            current_is_whitespace = Some(is_whitespace);
            continue;
        }
        parts.push((
            current_is_whitespace.unwrap_or(false),
            std::mem::take(&mut current),
        ));
        current.push(ch);
        current_is_whitespace = Some(is_whitespace);
    }
    if !current.is_empty() {
        parts.push((current_is_whitespace.unwrap_or(false), current));
    }
    parts
}

// Token-level redaction intentionally covers common credential shapes only. It
// avoids parsing arbitrary command output while masking values likely to contain
// auth material in SSH/rsync/rclone/HTTP errors.
fn redact_error_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if let Ok(mut url) = url::Url::parse(token.trim_matches(['\'', '"', ',', ';'])) {
            if !url.username().is_empty() || url.password().is_some() {
                let _ = url.set_username("REDACTED");
                let _ = url.set_password(None);
                return url.to_string();
            }
        }
    }
    for marker in [
        "token=",
        "password=",
        "secret=",
        "key=",
        "--token=",
        "--password=",
        "--secret=",
        "--key=",
    ] {
        if lower.starts_with(marker) {
            return format!("{marker}REDACTED");
        }
    }
    token.to_string()
}

async fn retry_entry(
    cfg: &Config,
    manifest_id: &str,
    keep_local: bool,
) -> Result<TransferQueueEntry> {
    let queue_dir = queue_dir(cfg);
    validate_manifest_id(manifest_id)?;
    let manifest_path = queue_dir.join(format!("{manifest_id}.json"));
    let mut entry = {
        let _lock = QueueLock::acquire(&queue_dir)?;
        let mut entry = read_manifest(&manifest_path)?;
        if entry.manifest_id != manifest_id {
            bail!("transfer queue manifest id does not match file name");
        }
        entry.attempts = entry.attempts.saturating_add(1);
        entry.updated_at = timestamp_now();
        let staging_path = PathBuf::from(&entry.staging_path);
        if !staging_path.is_dir() {
            mark_entry_pending(
                &mut entry,
                format!("staging directory no longer exists for manifest {manifest_id}"),
            );
            write_manifest(&entry)?;
            return Ok(entry);
        }
        entry.status = "running".to_string();
        write_manifest(&entry)?;
        entry
    };
    let staging_path = PathBuf::from(&entry.staging_path);
    let validation_entry = entry.clone();
    let validation_staging = staging_path.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || {
        validate_staged_files(&validation_entry, &validation_staging)
    })
    .await?
    {
        let _lock = QueueLock::acquire(&queue_dir)?;
        mark_entry_pending(&mut entry, error.to_string());
        write_manifest(&entry)?;
        return Ok(entry);
    }

    match drain_entry(cfg, &entry, &staging_path).await {
        Ok(()) => {
            let _lock = QueueLock::acquire(&queue_dir)?;
            entry.status = "completed".to_string();
            entry.last_error = None;
            if !keep_local {
                fs::remove_dir_all(&staging_path).with_context(|| {
                    format!(
                        "remove drained staging directory {}",
                        staging_path.display()
                    )
                })?;
            }
            fs::remove_file(&manifest_path).with_context(|| {
                format!("remove transfer queue manifest {}", manifest_path.display())
            })?;
            Ok(entry)
        }
        Err(error) => {
            let _lock = QueueLock::acquire(&queue_dir)?;
            mark_entry_pending(&mut entry, error.to_string());
            write_manifest(&entry)?;
            Ok(entry)
        }
    }
}

fn mark_entry_pending(entry: &mut TransferQueueEntry, error: String) {
    entry.status = "pending".to_string();
    entry.updated_at = timestamp_now();
    entry.last_error = Some(redact_transfer_error(&error));
}

fn validate_staged_files(entry: &TransferQueueEntry, staging_path: &Path) -> Result<()> {
    if entry.files.is_empty() {
        bail!("transfer queue manifest does not record any staged files");
    }
    let target_kinds: BTreeSet<&str> = entry
        .targets
        .iter()
        .map(|target| target.kind.as_str())
        .collect();
    let recorded = recorded_file_set(entry, &target_kinds)?;
    let actual = actual_file_set(staging_path)?;
    if recorded != actual {
        let missing = path_set_preview(recorded.difference(&actual));
        let extra = path_set_preview(actual.difference(&recorded));
        bail!("staged files no longer match transfer queue manifest: missing=[{missing}], extra=[{extra}]");
    }
    Ok(())
}

fn recorded_file_set(
    entry: &TransferQueueEntry,
    target_kinds: &BTreeSet<&str>,
) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for file in &entry.files {
        let path = PathBuf::from(file);
        ensure_relative_manifest_path(&path)?;
        let Some(kind) = path.components().next().and_then(|part| match part {
            Component::Normal(kind) => kind.to_str(),
            _ => None,
        }) else {
            bail!("transfer queue manifest contains an invalid staged file path");
        };
        if !target_kinds.contains(kind) {
            bail!("transfer queue manifest records {kind} file without a matching transfer target");
        }
        files.insert(path);
    }
    Ok(files)
}

fn actual_file_set(staging_path: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    collect_staged_files(staging_path, staging_path, &mut files)?;
    Ok(files)
}

fn path_set_preview<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> String {
    paths
        .take(5)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn collect_staged_files(root: &Path, dir: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(dir).with_context(|| format!("read staged directory {}", dir.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_staged_files(root, &path, files)?;
        } else if path.is_file() {
            files.insert(
                path.strip_prefix(root)
                    .with_context(|| format!("relativize staged file {}", path.display()))?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn ensure_relative_manifest_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        bail!("transfer queue manifest contains an absolute staged file path");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => bail!("transfer queue manifest contains an unsafe staged file path"),
        }
    }
    Ok(())
}

async fn drain_entry(cfg: &Config, entry: &TransferQueueEntry, staging_path: &Path) -> Result<()> {
    let ssh_opts = cfg.all_ssh_opts();
    let mut transferred_any = false;
    for target in &entry.targets {
        let parsed = crate::transfer::TargetPath::parse(&target.target_path)?;
        if matches!(parsed, crate::transfer::TargetPath::Local(_)) && !cfg.allow_local_targets {
            bail!(
                "Local target paths are disabled. Set YTDLP_ALLOW_LOCAL_TARGETS=true to allow local filesystem destinations."
            );
        }
        let kind_dir = staging_path.join(&target.kind);
        if !kind_dir.is_dir() {
            continue;
        }
        transferred_any = true;
        let transfer = async {
            crate::transfer::ensure_target_dir(&parsed, &ssh_opts).await?;
            crate::transfer::transfer_to_target(&kind_dir, &parsed, &ssh_opts).await
        };
        match tokio::time::timeout(cfg.transfer_timeout(), transfer).await {
            Ok(result) => result?,
            Err(_) => bail!(
                "transfer of {} timed out after {}s",
                target.kind,
                cfg.transfer_timeout().as_secs()
            ),
        }
    }
    if !transferred_any {
        bail!("no staged target directories were found for transfer queue manifest");
    }
    Ok(())
}

fn read_entries_unlocked(queue_dir: &Path) -> Result<Vec<TransferQueueEntry>> {
    let mut entries = Vec::new();
    if queue_dir.is_dir() {
        for dir_entry in fs::read_dir(queue_dir)? {
            let path = dir_entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                entries.push(read_manifest(&path)?);
            }
        }
    }
    entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(entries)
}

fn write_manifest(entry: &TransferQueueEntry) -> Result<()> {
    let parent = entry
        .manifest_path
        .parent()
        .context("manifest path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = entry.manifest_path.with_extension("json.tmp");
    {
        let mut file = File::create(&temp)
            .with_context(|| format!("create temp transfer manifest {}", temp.display()))?;
        serde_json::to_writer_pretty(&mut file, entry)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&temp, &entry.manifest_path).with_context(|| {
        format!(
            "replace transfer queue manifest {}",
            entry.manifest_path.display()
        )
    })?;
    #[cfg(unix)]
    {
        let dir = File::open(parent)?;
        dir.sync_all()?;
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<TransferQueueEntry> {
    let file =
        File::open(path).with_context(|| format!("open transfer manifest {}", path.display()))?;
    let mut entry: TransferQueueEntry = serde_json::from_reader(file)
        .with_context(|| format!("parse transfer manifest {}", path.display()))?;
    entry.manifest_path = path.to_path_buf();
    if entry.version != MANIFEST_VERSION {
        bail!(
            "unsupported transfer queue manifest version {}",
            entry.version
        );
    }
    validate_manifest_entry(&entry)?;
    Ok(entry)
}

fn validate_manifest_entry(entry: &TransferQueueEntry) -> Result<()> {
    match entry.status.as_str() {
        "pending" | "running" | "completed" => {}
        _ => bail!("transfer queue manifest contains an invalid status"),
    }
    if entry.targets.is_empty() {
        bail!("transfer queue manifest must contain at least one target");
    }
    let mut kinds = BTreeSet::new();
    for target in &entry.targets {
        match target.kind.as_str() {
            "audio" | "video" => {}
            _ => bail!("transfer queue manifest contains an unsupported target kind"),
        }
        if !kinds.insert(target.kind.as_str()) {
            bail!("transfer queue manifest contains duplicate target kinds");
        }
    }
    Ok(())
}

fn manifest_id(staging_path: &Path, created_at: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(staging_path.display().to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(created_at.as_bytes());
    let hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("tq_{hex}")
}

fn validate_manifest_id(manifest_id: &str) -> Result<()> {
    let Some(hex) = manifest_id.strip_prefix("tq_") else {
        bail!("invalid transfer queue manifest id");
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid transfer queue manifest id");
    }
    Ok(())
}

fn timestamp_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

struct QueueLock(File);

impl QueueLock {
    fn acquire(queue_dir: &Path) -> Result<Self> {
        fs::create_dir_all(queue_dir)
            .with_context(|| format!("create transfer queue directory {}", queue_dir.display()))?;
        let lock_path = queue_dir.join(".lock");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("open transfer queue lock {}", lock_path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock transfer queue {}", lock_path.display()))?;
        Ok(Self(file))
    }
}

impl Drop for QueueLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
