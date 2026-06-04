"""Thin, predictable wrapper around the yt-dlp Python API.

Two entry points are exposed: :func:`probe` (metadata only, no download) and
:func:`fetch` (download audio/video/both into a staging directory). File paths
are read back from ``info['requested_downloads'][*]['filepath']`` so the final,
post-processed names are captured even after extraction or merging.

yt-dlp is imported lazily inside functions (never at module load) so that the
startup auto-updater can replace it on disk before the first import.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

# Lossless / passthrough codecs where a quality knob does not apply.
_NO_QUALITY = {"best", "flac", "wav"}

# A yt-dlp progress hook: receives the per-tick status dict.
ProgressHook = Callable[[dict], None]


@dataclass(slots=True)
class MediaFile:
    """A single file produced on disk."""

    path: Path
    kind: str  # "audio" | "video"
    size: int


@dataclass(slots=True)
class ItemResult:
    """Per-URL outcome."""

    url: str
    title: str | None = None
    video_id: str | None = None
    duration: float | None = None
    uploader: str | None = None
    is_playlist: bool = False
    files: list[MediaFile] = field(default_factory=list)
    error: str | None = None


@dataclass(slots=True)
class ProbeResult:
    """Metadata-only lookup result."""

    url: str
    title: str | None = None
    video_id: str | None = None
    duration: float | None = None
    uploader: str | None = None
    is_playlist: bool = False
    entry_count: int | None = None
    format_count: int | None = None
    error: str | None = None


def _base_opts(staging: Path, progress_cb: ProgressHook | None) -> dict:
    opts: dict = {
        # Folder is the artist: prefer the real `artist` tag (set by the title
        # parser or YouTube Music), then fall back to the channel/uploader.
        "outtmpl": {
            "default": str(staging)
            + "/%(artist,uploader,channel,creator|Unknown Artist)s/%(title)s [%(id)s].%(ext)s"
        },
        "quiet": True,
        "no_warnings": True,
        "noprogress": True,
        "ignoreerrors": False,
        # Sanitise reserved characters so names survive any downstream filesystem.
        "windowsfilenames": True,
    }
    if progress_cb is not None:
        opts["progress_hooks"] = [progress_cb]
    return opts


# Containers whose tags/cover art ffmpeg can write. WAV has no usable tag or
# cover-art support, so metadata/thumbnail embedding is skipped for it.
_TAGGABLE = {"mp3", "m4a", "opus", "flac", "mp4", "mkv"}


def _metadata_pps(codec: str) -> list[dict]:
    """Postprocessors that make files land in a media server cleanly:

    1. Parse a leading ``Artist - Title`` out of the video title (non-greedy,
       so only the first ' - ' splits). No-op when the title has no ' - ',
       which leaves YouTube Music / Topic-channel tags untouched.
    2. Embed title/artist/date tags into the file.
    3. Embed the thumbnail as cover art.
    """
    from yt_dlp.postprocessor.metadataparser import MetadataParserPP

    pps: list[dict] = [
        {
            "key": "MetadataParser",
            "when": "pre_process",
            "actions": [
                (MetadataParserPP.Actions.INTERPRET, "title", r"(?P<artist>.+?) - (?P<title>.+)")
            ],
        }
    ]
    return pps


def _finalize_pps(opts: dict, codec: str) -> dict:
    """Append the tag + cover-art postprocessors (skipped for untaggable WAV)."""
    if codec not in _TAGGABLE:
        return opts
    opts.setdefault("postprocessors", [])
    opts["postprocessors"].append({"key": "FFmpegMetadata", "add_metadata": True})
    opts["writethumbnail"] = True
    opts["postprocessors"].append({"key": "EmbedThumbnail"})
    return opts


def _with_archive(opts: dict, archive_dir: Path | None, kind: str) -> dict:
    """Attach a per-kind download archive so 'both' mode tracks audio and video
    independently (a shared archive would skip the second pass)."""
    if archive_dir is not None:
        opts["download_archive"] = str(archive_dir / f"archive-{kind}.txt")
    return opts


def _audio_opts(
    staging: Path,
    audio_format: str,
    audio_quality: str,
    archive_dir: Path | None,
    progress_cb: ProgressHook | None,
) -> dict:
    opts = _base_opts(staging, progress_cb)
    opts["format"] = "bestaudio/best"
    extract: dict[str, str] = {"key": "FFmpegExtractAudio", "preferredcodec": audio_format}
    if audio_format not in _NO_QUALITY:
        extract["preferredquality"] = audio_quality
    # Order: parse Artist - Title, extract audio, then tag + embed cover art.
    opts["postprocessors"] = [*_metadata_pps(audio_format), extract]
    _finalize_pps(opts, audio_format)
    return _with_archive(opts, archive_dir, "audio")


def _video_opts(
    staging: Path,
    container: str,
    max_height: int | None,
    archive_dir: Path | None,
    progress_cb: ProgressHook | None,
) -> dict:
    opts = _base_opts(staging, progress_cb)
    height = f"[height<=?{max_height}]" if max_height else ""
    opts["format"] = f"bv*{height}+ba/b{height}"
    opts["merge_output_format"] = container
    # Same Artist - Title parse + tag + cover-art treatment as audio.
    opts["postprocessors"] = list(_metadata_pps(container))
    _finalize_pps(opts, container)
    return _with_archive(opts, archive_dir, "video")


def _entries(info: dict) -> list[dict]:
    entries = info.get("entries")
    if entries:
        return [e for e in entries if e]
    return [info]


def _files_from_info(info: dict, kind: str) -> list[MediaFile]:
    files: list[MediaFile] = []
    for entry in _entries(info):
        downloads = entry.get("requested_downloads") or []
        for download in downloads:
            raw = download.get("filepath") or download.get("_filename")
            if not raw:
                continue
            path = Path(raw)
            if path.exists():
                files.append(MediaFile(path=path, kind=kind, size=path.stat().st_size))
    return files


def _meta(info: dict) -> dict:
    if info.get("entries"):
        first = next((e for e in info["entries"] if e), {})
        return {
            "title": info.get("title") or first.get("playlist") or first.get("title"),
            "video_id": info.get("id"),
            "duration": None,
            "uploader": info.get("uploader") or first.get("uploader"),
            "is_playlist": True,
        }
    return {
        "title": info.get("title"),
        "video_id": info.get("id"),
        "duration": info.get("duration"),
        "uploader": info.get("uploader"),
        "is_playlist": False,
    }


def _run(url: str, opts: dict, kind: str) -> tuple[dict, list[MediaFile]]:
    import yt_dlp  # lazy: see module docstring

    with yt_dlp.YoutubeDL(opts) as ydl:
        info = ydl.extract_info(url, download=True)
    return info, _files_from_info(info, kind)


def fetch(
    url: str,
    mode: str,
    *,
    staging: Path,
    audio_format: str,
    audio_quality: str,
    container: str,
    max_height: int | None,
    archive_dir: Path | None = None,
    progress_cb: ProgressHook | None = None,
) -> ItemResult:
    """Download one URL according to ``mode`` and return its result.

    ``mode`` of ``both`` performs two passes (video, then audio) so the caller
    gets a merged video file *and* a separately extracted audio file. When an
    archive is enabled, already-downloaded items simply yield no new files.
    """
    from yt_dlp.utils import DownloadError  # lazy

    files: list[MediaFile] = []
    info: dict | None = None
    try:
        if mode in ("video", "both"):
            info, video_files = _run(
                url,
                _video_opts(staging / "video", container, max_height, archive_dir, progress_cb),
                "video",
            )
            files.extend(video_files)
        if mode in ("audio", "both"):
            audio_info, audio_files = _run(
                url,
                _audio_opts(staging / "audio", audio_format, audio_quality, archive_dir, progress_cb),
                "audio",
            )
            info = info or audio_info
            files.extend(audio_files)
    except DownloadError as exc:
        return ItemResult(url=url, error=str(exc).strip())
    except Exception as exc:  # noqa: BLE001 - surface anything actionable to the agent
        return ItemResult(url=url, error=f"{type(exc).__name__}: {exc}")

    return ItemResult(url=url, files=files, **_meta(info or {}))


def probe(url: str) -> ProbeResult:
    """Resolve metadata for a URL without downloading anything."""
    import yt_dlp  # lazy
    from yt_dlp.utils import DownloadError

    opts = {"quiet": True, "no_warnings": True, "skip_download": True}
    try:
        with yt_dlp.YoutubeDL(opts) as ydl:
            info = ydl.extract_info(url, download=False)
    except DownloadError as exc:
        return ProbeResult(url=url, error=str(exc).strip())
    except Exception as exc:  # noqa: BLE001
        return ProbeResult(url=url, error=f"{type(exc).__name__}: {exc}")

    entry_count = len([e for e in info["entries"] if e]) if info.get("entries") else None
    format_count = len(info.get("formats") or []) or None
    return ProbeResult(
        url=url,
        entry_count=entry_count,
        format_count=format_count,
        **_meta(info),
    )
