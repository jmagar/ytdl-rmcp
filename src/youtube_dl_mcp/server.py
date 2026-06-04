"""FastMCP server exposing yt-dlp download + rsync-to-remote tools."""

from __future__ import annotations

import asyncio
import importlib.util
import json
import shutil
import sys
import tempfile
from pathlib import Path

from mcp.server.fastmcp import Context, FastMCP

from . import downloader as dl
from . import transfer as tx
from . import updater
from .config import SETTINGS
from .models import DownloadInput, ProbeInput, ResponseFormat

mcp = FastMCP("youtube_dl_mcp")

# External binaries required at call time, mapped to an install hint.
_REQUIRED_BINARIES = {
    "rsync": "rsync (e.g. `apt install rsync`)",
    "ssh": "openssh-client",
    "ffmpeg": "ffmpeg (needed for audio extraction and video merging)",
}

# Only report download progress when the percentage moves at least this much.
_PROGRESS_STEP = 5


# --------------------------------------------------------------------------- #
# Small helpers
# --------------------------------------------------------------------------- #
def _missing_dependencies() -> list[str]:
    missing = [hint for binary, hint in _REQUIRED_BINARIES.items() if shutil.which(binary) is None]
    if importlib.util.find_spec("yt_dlp") is None:
        missing.append("yt-dlp (Python package)")
    return missing


def _human_size(num_bytes: int) -> str:
    size = float(num_bytes)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if size < 1024 or unit == "TiB":
            return f"{size:.1f} {unit}" if unit != "B" else f"{int(size)} B"
        size /= 1024
    return f"{size:.1f} TiB"


def _human_duration(seconds: float | None) -> str:
    if not seconds:
        return "unknown"
    total = int(seconds)
    hours, rem = divmod(total, 3600)
    minutes, secs = divmod(rem, 60)
    return f"{hours}:{minutes:02d}:{secs:02d}" if hours else f"{minutes}:{secs:02d}"


async def _log(ctx: Context, message: str) -> None:
    try:
        await ctx.info(message)
    except Exception:  # noqa: BLE001 - logging must never break a tool
        pass


async def _progress(ctx: Context, progress: float, message: str) -> None:
    try:
        await ctx.report_progress(progress=progress, total=1.0, message=message)
    except Exception:  # noqa: BLE001
        pass


def _error(message: str) -> str:
    return f"Error: {message}"


def _make_progress_hook(ctx: Context, loop: asyncio.AbstractEventLoop) -> dl.ProgressHook:
    """Build a yt-dlp progress hook that bridges from the download worker thread
    back to the event loop. Throttled so it doesn't flood the client."""
    state = {"last_pct": -_PROGRESS_STEP}

    def submit(progress: float, message: str) -> None:
        try:
            asyncio.run_coroutine_threadsafe(_progress(ctx, progress, message), loop)
        except Exception:  # noqa: BLE001 - loop may be shutting down
            pass

    def hook(status: dict) -> None:
        try:
            phase = status.get("status")
            name = Path(status.get("filename") or "").name
            if phase == "downloading":
                total = status.get("total_bytes") or status.get("total_bytes_estimate") or 0
                done = status.get("downloaded_bytes") or 0
                pct = int(done * 100 / total) if total else 0
                if pct - state["last_pct"] < _PROGRESS_STEP:
                    return
                state["last_pct"] = pct
                frac = (done / total) if total else 0.0
                # Map download into the 0.1 - 0.6 band of overall progress.
                submit(0.1 + 0.5 * frac, f"Downloading {name}: {pct}%")
            elif phase == "finished":
                state["last_pct"] = -_PROGRESS_STEP  # reset for the next file
                submit(0.6, f"Downloaded {name}, post-processing...")
        except Exception:  # noqa: BLE001
            pass

    return hook


# --------------------------------------------------------------------------- #
# Formatting
# --------------------------------------------------------------------------- #
def _download_payload(
    results: list[dl.ItemResult],
    remote: str,
    dest_path: str,
    transferred: bool,
    transfer_error: str | None,
    staging_kept: Path | None,
) -> dict:
    items = []
    for result in results:
        items.append(
            {
                "url": result.url,
                "title": result.title,
                "video_id": result.video_id,
                "duration": result.duration,
                "uploader": result.uploader,
                "is_playlist": result.is_playlist,
                "error": result.error,
                "files": [
                    {"name": f.path.name, "kind": f.kind, "bytes": f.size}
                    for f in result.files
                ],
            }
        )
    total_files = sum(len(r.files) for r in results)
    total_bytes = sum(f.size for r in results for f in r.files)
    return {
        "transferred": transferred,
        "transfer_error": transfer_error,
        "remote": remote,
        "dest_path": dest_path,
        "destination": f"{remote}:{dest_path}",
        "staging_kept_at": str(staging_kept) if staging_kept else None,
        "total_files": total_files,
        "total_bytes": total_bytes,
        "total_size": _human_size(total_bytes),
        "items": items,
    }


def _render_download_markdown(payload: dict) -> str:
    lines: list[str] = []
    if payload["transferred"]:
        lines.append(
            f"Transferred {payload['total_files']} file(s) "
            f"({payload['total_size']}) to `{payload['destination']}`."
        )
    else:
        lines.append(f"Download succeeded but transfer failed: {payload['transfer_error']}")
        if payload["staging_kept_at"]:
            lines.append(f"Local files kept at `{payload['staging_kept_at']}` for retry.")
    lines.append("")

    for item in payload["items"]:
        if item["error"]:
            lines.append(f"- {item['url']} - failed: {item['error']}")
            continue
        title = item["title"] or item["url"]
        suffix = " (playlist)" if item["is_playlist"] else ""
        if not item["files"]:
            lines.append(f"- {title}{suffix} - nothing new (already archived)")
            continue
        lines.append(f"- {title}{suffix}")
        for file in item["files"]:
            lines.append(f"    - [{file['kind']}] {file['name']} ({_human_size(file['bytes'])})")
    return "\n".join(lines).strip()


def _render_probe_markdown(results: list[dl.ProbeResult]) -> str:
    lines: list[str] = []
    for result in results:
        if result.error:
            lines.append(f"- {result.url} - failed: {result.error}")
            continue
        title = result.title or result.url
        if result.is_playlist:
            lines.append(f"- {title} - playlist, {result.entry_count} item(s)")
        else:
            lines.append(
                f"- {title} - {_human_duration(result.duration)}"
                + (f", by {result.uploader}" if result.uploader else "")
                + (f", {result.format_count} formats" if result.format_count else "")
            )
    return "\n".join(lines).strip()


def _probe_payload(results: list[dl.ProbeResult]) -> dict:
    return {
        "items": [
            {
                "url": r.url,
                "title": r.title,
                "video_id": r.video_id,
                "duration": r.duration,
                "uploader": r.uploader,
                "is_playlist": r.is_playlist,
                "entry_count": r.entry_count,
                "format_count": r.format_count,
                "error": r.error,
            }
            for r in results
        ]
    }


# --------------------------------------------------------------------------- #
# Blocking worker (runs off the event loop)
# --------------------------------------------------------------------------- #
def _download_all(
    params: DownloadInput,
    staging: Path,
    archive_dir: Path | None,
    progress_cb: dl.ProgressHook | None,
) -> list[dl.ItemResult]:
    return [
        dl.fetch(
            url,
            params.mode.value,
            staging=staging,
            audio_format=params.audio_format.value,
            audio_quality=params.audio_quality,
            container=params.container.value,
            max_height=params.max_height,
            archive_dir=archive_dir,
            progress_cb=progress_cb,
        )
        for url in params.urls
    ]


# --------------------------------------------------------------------------- #
# Tools
# --------------------------------------------------------------------------- #
@mcp.tool(
    name="youtube_download",
    annotations={
        "title": "Download media and rsync to a remote",
        "readOnlyHint": False,
        "destructiveHint": False,
        "idempotentHint": False,
        "openWorldHint": True,
    },
)
async def youtube_download(params: DownloadInput, ctx: Context) -> str:
    """Download audio, video, or both from one or more URLs with yt-dlp, then
    rsync the resulting files to a directory on an SSH remote.

    Files are staged locally, transferred, and (by default) the local copy is
    removed. If the transfer fails the local staging copy is preserved so the
    operation can be retried. ``mode='both'`` produces a merged video file and a
    separately extracted audio file per source. With ``use_archive=True``,
    previously downloaded items are skipped on subsequent calls.

    Args:
        params (DownloadInput): Validated parameters:
            - urls (list[str]): Source URLs (a single string is accepted).
            - mode (DownloadMode): 'audio' (default), 'video', or 'both'.
            - audio_format (AudioFormat): Codec for audio; 'best' skips re-encode.
            - audio_quality (str): Quality for lossy codecs ('0'-'9' or '192K').
            - max_height (int | None): Cap video resolution (e.g. 1080).
            - container (VideoContainer): 'mp4' (default) or 'mkv' for video.
            - remote (str | None): SSH alias or user@host (or env YTDLP_REMOTE).
            - dest_path (str | None): Absolute remote dir (or env YTDLP_REMOTE_PATH).
            - keep_local (bool): Keep the staged local copy after transfer.
            - use_archive (bool): Skip already-downloaded items on repeat calls.
            - response_format (ResponseFormat): 'markdown' (default) or 'json'.

    Returns:
        str: A summary listing each item, the files produced (name, kind, size),
        and the remote destination. Items skipped via the archive are noted.
        On transfer failure, includes the local staging path. JSON form
        additionally exposes byte counts and IDs.
    """
    remote = params.remote or SETTINGS.remote
    audio_dest = params.dest_path or SETTINGS.dest_path
    video_dest = params.video_dest_path or SETTINGS.video_dest_path or audio_dest
    if not remote:
        return _error("No SSH remote. Pass 'remote' or set the YTDLP_REMOTE env var.")
    if not audio_dest:
        return _error("No destination. Pass 'dest_path' or set YTDLP_REMOTE_PATH.")

    missing = _missing_dependencies()
    if missing:
        return _error("Missing dependencies: " + ", ".join(missing))

    archive_dir: Path | None = None
    if params.use_archive:
        archive_dir = Path(SETTINGS.archive_dir)
        archive_dir.mkdir(parents=True, exist_ok=True)

    loop = asyncio.get_running_loop()
    progress_cb = _make_progress_hook(ctx, loop)

    staging = Path(tempfile.mkdtemp(prefix="ytdlmcp_", dir=SETTINGS.staging_dir))
    # Targets keyed by kind: subdir of staging → remote dest.
    kind_targets: dict[str, tuple[Path, str]] = {
        "audio": (staging / "audio", audio_dest),
        "video": (staging / "video", video_dest),
    }
    cleanup = not params.keep_local
    transferred = False
    transfer_error: str | None = None
    # For summary reporting — primary dest is audio (covers the common case).
    primary_dest = audio_dest if params.mode.value in ("audio", "both") else video_dest

    try:
        await _log(ctx, f"Downloading {len(params.urls)} item(s) in mode '{params.mode.value}'.")
        await _progress(ctx, 0.1, "Downloading")
        results = await asyncio.to_thread(
            _download_all, params, staging, archive_dir, progress_cb
        )

        files = [f.path for r in results for f in r.files]
        if not files:
            if any(r.error for r in results):
                errors = "; ".join(r.error for r in results if r.error)
                return _error(f"Nothing was downloaded: {errors}")
            payload = _download_payload(
                results, remote, primary_dest, transferred=True,
                transfer_error=None, staging_kept=None,
            )
            if cleanup:
                shutil.rmtree(staging, ignore_errors=True)
            if params.response_format is ResponseFormat.JSON:
                return json.dumps(payload, indent=2)
            return _render_download_markdown(payload)

        await _progress(ctx, 0.7, "Transferring to remote")
        for kind, (kind_dir, dest) in kind_targets.items():
            if not kind_dir.exists():
                continue
            await _log(ctx, f"Transferring {kind} to {remote}:{dest}")
            await asyncio.to_thread(tx.ensure_remote_dir, remote, dest, SETTINGS.ssh_opts)
            await asyncio.to_thread(tx.rsync, kind_dir, remote, dest, SETTINGS.ssh_opts)

        transferred = True
        await _progress(ctx, 1.0, "Done")
    except tx.TransferError as exc:
        cleanup = False
        transfer_error = str(exc)
        await _log(ctx, transfer_error)
    finally:
        if cleanup:
            shutil.rmtree(staging, ignore_errors=True)

    payload = _download_payload(
        results,
        remote=remote,
        dest_path=primary_dest,
        transferred=transferred,
        transfer_error=transfer_error,
        staging_kept=None if cleanup else staging,
    )
    if params.response_format is ResponseFormat.JSON:
        return json.dumps(payload, indent=2)
    return _render_download_markdown(payload)


@mcp.tool(
    name="youtube_probe",
    annotations={
        "title": "Inspect media metadata (no download)",
        "readOnlyHint": True,
        "destructiveHint": False,
        "idempotentHint": True,
        "openWorldHint": True,
    },
)
async def youtube_probe(params: ProbeInput, ctx: Context) -> str:
    """Resolve title, duration, uploader, and format/entry counts for URLs
    without downloading. Useful to confirm a target before a large download.

    Args:
        params (ProbeInput): Validated parameters:
            - urls (list[str]): Source URLs (a single string is accepted).
            - response_format (ResponseFormat): 'markdown' (default) or 'json'.

    Returns:
        str: Per-URL metadata. For playlists, the entry count is reported; for
        single items, duration/uploader/format count.
    """
    if importlib.util.find_spec("yt_dlp") is None:
        return _error("Missing dependency: yt-dlp (Python package).")

    await _log(ctx, f"Probing {len(params.urls)} URL(s).")
    results = await asyncio.to_thread(lambda: [dl.probe(u) for u in params.urls])

    if params.response_format is ResponseFormat.JSON:
        return json.dumps(_probe_payload(results), indent=2)
    return _render_probe_markdown(results)


def main() -> None:
    """Console-script entry point.

    Transport is selected via MCP_TRANSPORT env var:
      stdio             (default) — JSON-RPC over stdin/stdout
      streamable-http   — HTTP server; MCP_HOST / MCP_PORT control binding

    Auto-updates yt-dlp at startup when stale. Disable with YTDLP_AUTO_UPDATE=0.
    """
    import os

    if SETTINGS.auto_update:
        status = updater.ensure_fresh(
            SETTINGS.max_age_days, SETTINGS.update_pre, respect_interval=False
        )
        print(f"[youtube-dl-mcp] yt-dlp auto-update: {status}", file=sys.stderr)

    transport = os.environ.get("MCP_TRANSPORT", "stdio")
    if transport == "streamable-http":
        from mcp.server.transport_security import TransportSecuritySettings

        host = os.environ.get("MCP_HOST", "0.0.0.0")
        port = int(os.environ.get("MCP_PORT", "40090"))
        print(f"[youtube-dl-mcp] HTTP mode on {host}:{port}", file=sys.stderr)
        mcp.settings.host = host
        mcp.settings.port = port
        # Allow any Host header so the server works behind Docker/proxies.
        mcp.settings.transport_security = TransportSecuritySettings(
            enable_dns_rebinding_protection=False
        )
    mcp.run(transport=transport)  # type: ignore[arg-type]


if __name__ == "__main__":
    main()
