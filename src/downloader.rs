//! yt-dlp invocation: build the argv, run the subprocess, collect produced
//! files + metadata. Ports `downloader.py`.
//!
//! yt-dlp does the heavy lifting; this module just constructs the right flags
//! and reads back what it produced via `--print after_move:`.

use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{bail, Result};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::bootstrap::Tools;
use crate::model::{AudioFormat, DownloadMode, SearchResultItem, VideoContainer};

/// Field separator embedded in the `--print` template (unit separator, unlikely
/// to appear in titles).
const SEP: char = '\u{1f}';
const STDERR_TAIL_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct MediaFile {
    pub path: PathBuf,
    pub kind: &'static str, // "audio" | "video"
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ItemResult {
    pub url: String,
    pub title: Option<String>,
    pub video_id: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub is_playlist: bool,
    pub files: Vec<MediaFile>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct FetchOptions<'a> {
    pub mode: DownloadMode,
    pub staging: &'a Path,
    pub audio_format: AudioFormat,
    pub audio_quality: &'a str,
    pub container: VideoContainer,
    pub max_height: Option<u32>,
    pub archive_dir: Option<&'a Path>,
    pub timeout: Option<Duration>,
}

#[derive(Debug)]
struct CommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: String,
}

/// Non-greedy "Artist - Title" split applied to the title field so the artist
/// populates tags + the output folder. No-op when the title has no " - ".
const PARSE_ARTIST: &str = r"title:(?P<artist>.+?) - (?P<title>.+)";

/// Output template: per-kind subdir / Artist / Title [id].ext.
fn output_template(staging: &Path, kind: &str) -> String {
    format!(
        "{}/{kind}/%(artist,uploader,channel,creator|Unknown Artist)s/%(title)s [%(id)s].%(ext)s",
        staging.display()
    )
}

/// The `--print` template emitted once per produced file, after the final move.
fn print_template() -> String {
    format!("after_move:%(id)s{SEP}%(title)s{SEP}%(uploader)s{SEP}%(duration)s{SEP}%(filepath)s")
}

fn common_args(staging: &Path, kind: &str, tools: &Tools, archive: Option<&Path>) -> Vec<String> {
    let mut a = vec![
        "--quiet".into(),
        "--no-warnings".into(),
        "--no-progress".into(),
        "--windows-filenames".into(),
        "--parse-metadata".into(),
        PARSE_ARTIST.into(),
        "-o".into(),
        output_template(staging, kind),
        "--print".into(),
        print_template(),
    ];
    if let Some(dir) = &tools.ffmpeg_dir {
        a.push("--ffmpeg-location".into());
        a.push(dir.display().to_string());
    }
    if let Some(extra) = &tools.extractor_args {
        a.push("--extractor-args".into());
        a.push(extra.clone());
    }
    if let Some(arch) = archive {
        a.push("--download-archive".into());
        a.push(arch.display().to_string());
    }
    a
}

fn audio_args(
    staging: &Path,
    fmt: AudioFormat,
    quality: &str,
    tools: &Tools,
    archive: Option<&Path>,
) -> Vec<String> {
    let mut a = common_args(staging, "audio", tools, archive);
    a.extend([
        "-f".into(),
        "bestaudio/best".into(),
        "--extract-audio".into(),
        "--audio-format".into(),
        fmt.as_str().into(),
    ]);
    if !fmt.is_lossless_or_passthrough() {
        a.push("--audio-quality".into());
        a.push(quality.to_string());
    }
    if fmt.is_taggable() {
        a.extend(["--embed-metadata".into(), "--embed-thumbnail".into()]);
    }
    a
}

fn video_args(
    staging: &Path,
    container: VideoContainer,
    max_height: Option<u32>,
    tools: &Tools,
    archive: Option<&Path>,
) -> Vec<String> {
    let mut a = common_args(staging, "video", tools, archive);
    let h = max_height
        .map(|h| format!("[height<=?{h}]"))
        .unwrap_or_default();
    a.extend([
        "-f".into(),
        format!("bv*{h}+ba/b{h}"),
        "--merge-output-format".into(),
        container.as_str().into(),
        "--embed-metadata".into(),
        "--embed-thumbnail".into(),
    ]);
    a
}

/// Run one yt-dlp pass, parsing the produced files + metadata from its
/// `--print` output straight into `result`. Fields are filled once (first
/// pass wins); `is_playlist` is set when a single pass yields >1 file.
async fn run_pass(
    ytdlp: &Path,
    url: &str,
    args: Vec<String>,
    kind: &'static str,
    timeout: Option<Duration>,
    result: &mut ItemResult,
) -> Result<()> {
    let mut cmd = Command::new(ytdlp);
    cmd.args(&args).arg(url);
    let output = run_command(&mut cmd, timeout).await?;
    if !output.status.success() {
        bail!("{}", command_error_text(&output.stderr, &output.stdout));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut count = 0;
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(SEP).collect();
        if parts.len() != 5 {
            continue;
        }
        let (id, title, uploader, duration, filepath) =
            (parts[0], parts[1], parts[2], parts[3], parts[4]);
        result.video_id.get_or_insert_with(|| id.to_string());
        result.title.get_or_insert_with(|| title.to_string());
        if result.uploader.is_none() && uploader != "NA" {
            result.uploader = Some(uploader.to_string());
        }
        if result.duration.is_none() {
            result.duration = duration.parse().ok();
        }
        let path = PathBuf::from(filepath);
        if let Ok(md) = tokio::fs::metadata(&path).await {
            result.files.push(MediaFile {
                path,
                kind,
                size: md.len(),
            });
            count += 1;
        }
    }
    if count > 1 {
        result.is_playlist = true;
    }
    Ok(())
}

/// Download one URL per `mode`, returning the per-URL outcome. `mode = both`
/// runs two passes (video then audio) into their own staging subdirs.
#[allow(clippy::too_many_arguments)]
pub async fn fetch(tools: &Tools, url: &str, options: FetchOptions<'_>) -> ItemResult {
    let mut result = ItemResult {
        url: url.to_string(),
        ..Default::default()
    };

    if matches!(options.mode, DownloadMode::Video | DownloadMode::Both) {
        let archive = options.archive_dir.map(|d| d.join("archive-video.txt"));
        let args = video_args(
            options.staging,
            options.container,
            options.max_height,
            tools,
            archive.as_deref(),
        );
        if let Err(e) = run_pass(
            &tools.ytdlp,
            url,
            args,
            "video",
            options.timeout,
            &mut result,
        )
        .await
        {
            result.error = Some(e.to_string());
            return result;
        }
    }
    if matches!(options.mode, DownloadMode::Audio | DownloadMode::Both) {
        let archive = options.archive_dir.map(|d| d.join("archive-audio.txt"));
        let args = audio_args(
            options.staging,
            options.audio_format,
            options.audio_quality,
            tools,
            archive.as_deref(),
        );
        if let Err(e) = run_pass(
            &tools.ytdlp,
            url,
            args,
            "audio",
            options.timeout,
            &mut result,
        )
        .await
        {
            result.error = Some(e.to_string());
            return result;
        }
    }
    result
}

#[derive(Debug, Clone, Default)]
pub struct ProbeResult {
    pub url: String,
    pub title: Option<String>,
    pub video_id: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub is_playlist: bool,
    pub entry_count: Option<usize>,
    pub format_count: Option<usize>,
    pub error: Option<String>,
}

/// Resolve metadata for a URL without downloading (`yt-dlp -J --skip-download`).
/// Takes just the yt-dlp path — probe never needs ffmpeg.
pub async fn probe(
    ytdlp: &Path,
    url: &str,
    extractor_args: Option<&str>,
    timeout: Option<Duration>,
) -> ProbeResult {
    let mut r = ProbeResult {
        url: url.to_string(),
        ..Default::default()
    };
    let mut cmd = Command::new(ytdlp);
    cmd.args(["-J", "--skip-download", "--no-warnings", "--quiet"]);
    if let Some(extra) = extractor_args {
        cmd.arg("--extractor-args").arg(extra);
    }
    cmd.arg(url);
    let output = match run_command(&mut cmd, timeout).await {
        Ok(o) => o,
        Err(e) => {
            r.error = Some(e.to_string());
            return r;
        }
    };
    if !output.status.success() {
        r.error = Some(output.stderr.trim().to_string());
        return r;
    }
    let info: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(e) => {
            r.error = Some(format!("could not parse yt-dlp JSON: {e}"));
            return r;
        }
    };

    let entries = info.get("entries").and_then(|e| e.as_array());
    if let Some(entries) = entries {
        let non_null: Vec<&serde_json::Value> = entries.iter().filter(|e| !e.is_null()).collect();
        r.is_playlist = true;
        r.entry_count = Some(non_null.len());
        r.title = str_field(&info, "title")
            .or_else(|| non_null.first().and_then(|e| str_field(e, "playlist")));
        r.video_id = str_field(&info, "id");
        r.uploader = str_field(&info, "uploader")
            .or_else(|| non_null.first().and_then(|e| str_field(e, "uploader")));
    } else {
        r.title = str_field(&info, "title");
        r.video_id = str_field(&info, "id");
        r.uploader = str_field(&info, "uploader");
        r.duration = info.get("duration").and_then(|d| d.as_f64());
        r.format_count = info
            .get("formats")
            .and_then(|f| f.as_array())
            .map(|a| a.len())
            .filter(|n| *n > 0);
    }
    r
}

pub(crate) fn parse_search_json(bytes: &[u8]) -> Result<Vec<SearchResultItem>> {
    let info: serde_json::Value = serde_json::from_slice(bytes)?;
    let Some(entries) = info.get("entries").and_then(|entries| entries.as_array()) else {
        let keys = info
            .as_object()
            .map(|object| object.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        anyhow::bail!(
            "yt-dlp search JSON did not contain an entries array; top-level keys: {keys:?}"
        );
    };

    let results = entries
        .iter()
        .filter_map(search_result_item)
        .collect::<Vec<_>>();

    Ok(results)
}

fn search_result_item(entry: &serde_json::Value) -> Option<SearchResultItem> {
    if entry.is_null() {
        return None;
    }
    let title = str_field(entry, "title")?;
    let url = search_result_url(entry)?;
    Some(SearchResultItem {
        title,
        url,
        video_id: str_field(entry, "id"),
        uploader: str_field(entry, "uploader").or_else(|| str_field(entry, "channel")),
        duration: entry.get("duration").and_then(|d| d.as_f64()),
        thumbnail: str_field(entry, "thumbnail"),
        view_count: entry.get("view_count").and_then(|v| v.as_u64()),
    })
}

fn search_result_url(entry: &serde_json::Value) -> Option<String> {
    if let Some(url) = str_field(entry, "webpage_url") {
        return Some(url);
    }
    if let Some(url) = str_field(entry, "url").filter(|url| is_http_url(url)) {
        return Some(url);
    }
    str_field(entry, "id").map(|id| format!("https://www.youtube.com/watch?v={id}"))
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

pub(crate) fn search_spec(query: &str, limit: u32) -> String {
    format!("ytsearch{}:{}", limit.clamp(1, 25), query.trim())
}

pub async fn search_youtube(
    ytdlp: &Path,
    query: &str,
    limit: u32,
    extractor_args: Option<&str>,
    timeout: Option<Duration>,
) -> Result<Vec<SearchResultItem>> {
    let mut cmd = Command::new(ytdlp);
    cmd.args([
        "--dump-single-json",
        "--skip-download",
        "--no-warnings",
        "--quiet",
    ]);
    if let Some(extra) = extractor_args {
        cmd.arg("--extractor-args").arg(extra);
    }
    cmd.arg(search_spec(query, limit));

    let output = run_command(&mut cmd, timeout).await?;
    if !output.status.success() {
        bail!("{}", command_error_text(&output.stderr, &output.stdout));
    }

    parse_search_json(&output.stdout)
}

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(ToOwned::to_owned)
        .filter(|s| !s.is_empty())
}

fn command_error_text(stderr: &str, stdout: &[u8]) -> String {
    let err = stderr.trim();
    if err.is_empty() {
        String::from_utf8_lossy(stdout).trim().to_string()
    } else {
        err.to_string()
    }
}

async fn run_command(cmd: &mut Command, timeout: Option<Duration>) -> Result<CommandOutput> {
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn()?;
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let stdout_task = tokio::spawn(async move {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).await.map(|_| out)
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = [0_u8; 8192];
        let mut tail = Vec::new();
        loop {
            let read = stderr.read(&mut buf).await?;
            if read == 0 {
                break;
            }
            append_tail(&mut tail, &buf[..read], STDERR_TAIL_BYTES);
        }
        Ok::<_, std::io::Error>(stderr_tail_text(&tail, STDERR_TAIL_BYTES))
    });

    let status = if let Some(limit) = timeout {
        match tokio::time::timeout(limit, child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                let _ = child.kill().await;
                bail!("command timed out after {}s", limit.as_secs());
            }
        }
    } else {
        child.wait().await?
    };

    let stdout = stdout_task.await??;
    let stderr = stderr_task.await??;
    Ok(CommandOutput {
        status,
        stdout,
        stderr,
    })
}

fn append_tail(tail: &mut Vec<u8>, chunk: &[u8], limit: usize) {
    tail.extend_from_slice(chunk);
    if tail.len() > limit {
        let excess = tail.len() - limit;
        tail.drain(..excess);
    }
}

fn stderr_tail_text(bytes: &[u8], limit: usize) -> String {
    let truncated = bytes.len() >= limit;
    let mut text = String::from_utf8_lossy(bytes).to_string();
    if truncated {
        if let Some(pos) = text.find('\n') {
            text.drain(..=pos);
        }
        text.insert_str(0, "[stderr truncated]\n");
    }
    text
}

#[cfg(test)]
#[path = "downloader_tests.rs"]
mod tests;
