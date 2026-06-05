//! yt-dlp invocation: build the argv, run the subprocess, collect produced
//! files + metadata. Ports `downloader.py`.
//!
//! yt-dlp does the heavy lifting; this module just constructs the right flags
//! and reads back what it produced via `--print after_move:`.

use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::process::Command;

use crate::bootstrap::Tools;
use crate::model::{AudioFormat, DownloadMode, VideoContainer};
use crate::util::command_error;

/// Field separator embedded in the `--print` template (unit separator, unlikely
/// to appear in titles).
const SEP: char = '\u{1f}';

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
    result: &mut ItemResult,
) -> Result<()> {
    let output = Command::new(ytdlp).args(&args).arg(url).output().await?;
    if !output.status.success() {
        anyhow::bail!("{}", command_error(&output));
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
pub async fn fetch(
    tools: &Tools,
    url: &str,
    mode: DownloadMode,
    staging: &Path,
    audio_format: AudioFormat,
    audio_quality: &str,
    container: VideoContainer,
    max_height: Option<u32>,
    archive_dir: Option<&Path>,
) -> ItemResult {
    let mut result = ItemResult {
        url: url.to_string(),
        ..Default::default()
    };

    if matches!(mode, DownloadMode::Video | DownloadMode::Both) {
        let archive = archive_dir.map(|d| d.join("archive-video.txt"));
        let args = video_args(staging, container, max_height, tools, archive.as_deref());
        if let Err(e) = run_pass(&tools.ytdlp, url, args, "video", &mut result).await {
            result.error = Some(e.to_string());
            return result;
        }
    }
    if matches!(mode, DownloadMode::Audio | DownloadMode::Both) {
        let archive = archive_dir.map(|d| d.join("archive-audio.txt"));
        let args = audio_args(
            staging,
            audio_format,
            audio_quality,
            tools,
            archive.as_deref(),
        );
        if let Err(e) = run_pass(&tools.ytdlp, url, args, "audio", &mut result).await {
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
pub async fn probe(ytdlp: &Path, url: &str, extractor_args: Option<&str>) -> ProbeResult {
    let mut r = ProbeResult {
        url: url.to_string(),
        ..Default::default()
    };
    let mut cmd = Command::new(ytdlp);
    cmd.args(["-J", "--skip-download", "--no-warnings", "--quiet"]);
    if let Some(extra) = extractor_args {
        cmd.arg("--extractor-args").arg(extra);
    }
    let output = match cmd.arg(url).output().await {
        Ok(o) => o,
        Err(e) => {
            r.error = Some(e.to_string());
            return r;
        }
    };
    if !output.status.success() {
        r.error = Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
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

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(ToOwned::to_owned)
        .filter(|s| !s.is_empty())
}
