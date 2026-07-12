#[cfg(test)]
#[path = "transfer_queue_tests.rs"]
mod tests;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

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
    let queue_dir = queue_dir(cfg);
    let _lock = QueueLock::acquire(&queue_dir)?;
    let entry = retry_entry_unlocked(cfg, &queue_dir, manifest_id, keep_local).await?;
    Ok(TransferQueueDrainResult {
        retried: 1,
        completed: usize::from(entry.status == "completed"),
        failed: usize::from(entry.status != "completed"),
        entries: vec![entry],
        errors: Vec::new(),
    })
}

pub(crate) async fn retry_all(cfg: &Config, keep_local: bool) -> Result<TransferQueueDrainResult> {
    let queue_dir = queue_dir(cfg);
    let _lock = QueueLock::acquire(&queue_dir)?;
    let ids: Vec<String> = read_entries_unlocked(&queue_dir)?
        .into_iter()
        .map(|entry| entry.manifest_id)
        .collect();
    let mut result = TransferQueueDrainResult {
        retried: 0,
        completed: 0,
        failed: 0,
        entries: Vec::new(),
        errors: Vec::new(),
    };
    for manifest_id in ids {
        result.retried += 1;
        match retry_entry_unlocked(cfg, &queue_dir, &manifest_id, keep_local).await {
            Ok(entry) if entry.status == "completed" => {
                result.completed += 1;
                result.entries.push(entry);
            }
            Ok(entry) => {
                result.failed += 1;
                result.entries.push(entry);
            }
            Err(error) => {
                result.failed += 1;
                result.errors.push(redact_transfer_error(&error.to_string()));
            }
        }
    }
    Ok(result)
}

pub(crate) fn queue_dir(cfg: &Config) -> PathBuf {
    let base = cfg
        .history_path
        .as_ref()
        .map(PathBuf::from)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| {
            crate::bootstrap::project_dirs()
                .map(|dirs| dirs.state_dir().unwrap_or_else(|| dirs.data_dir()).to_path_buf())
        })
        .unwrap_or_else(|| std::env::temp_dir().join("ytdl-rmcp-state"));
    base.join("transfer-queue")
}

pub(crate) fn redact_transfer_error(error: &str) -> String {
    let mut output = String::with_capacity(error.len());
    let mut token = String::new();
    for ch in error.chars() {
        if ch.is_whitespace() {
            output.push_str(&redact_error_token(&token));
            token.clear();
            output.push(ch);
        } else {
            token.push(ch);
        }
    }
    output.push_str(&redact_error_token(&token));
    output
}

fn redact_error_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    for marker in ["token=", "password=", "secret=", "key="] {
        if lower.starts_with(marker) {
            return format!("{marker}REDACTED");
        }
    }
    token.to_string()
}

async fn retry_entry_unlocked(
    cfg: &Config,
    queue_dir: &Path,
    manifest_id: &str,
    keep_local: bool,
) -> Result<TransferQueueEntry> {
    validate_manifest_id(manifest_id)?;
    let manifest_path = queue_dir.join(format!("{manifest_id}.json"));
    let mut entry = read_manifest(&manifest_path)?;
    let staging_path = PathBuf::from(&entry.staging_path);
    if !staging_path.is_dir() {
        bail!("staging directory no longer exists for manifest {manifest_id}");
    }

    entry.attempts = entry.attempts.saturating_add(1);
    entry.updated_at = timestamp_now();
    match drain_entry(cfg, &entry, &staging_path).await {
        Ok(()) => {
            entry.status = "completed".to_string();
            entry.last_error = None;
            fs::remove_file(&manifest_path)
                .with_context(|| format!("remove transfer queue manifest {}", manifest_path.display()))?;
            if !keep_local {
                fs::remove_dir_all(&staging_path).with_context(|| {
                    format!("remove drained staging directory {}", staging_path.display())
                })?;
            }
            Ok(entry)
        }
        Err(error) => {
            let redacted = redact_transfer_error(&error.to_string());
            entry.status = "pending".to_string();
            entry.last_error = Some(redacted.clone());
            write_manifest(&entry)?;
            bail!("{redacted}");
        }
    }
}

async fn drain_entry(cfg: &Config, entry: &TransferQueueEntry, staging_path: &Path) -> Result<()> {
    let ssh_opts = cfg.all_ssh_opts();
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
    let dir = File::open(parent)?;
    dir.sync_all()?;
    Ok(())
}

fn read_manifest(path: &Path) -> Result<TransferQueueEntry> {
    let file = File::open(path).with_context(|| format!("open transfer manifest {}", path.display()))?;
    let mut entry: TransferQueueEntry = serde_json::from_reader(file)
        .with_context(|| format!("parse transfer manifest {}", path.display()))?;
    entry.manifest_path = path.to_path_buf();
    if entry.version != MANIFEST_VERSION {
        bail!("unsupported transfer queue manifest version {}", entry.version);
    }
    Ok(entry)
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
