"""Pydantic input models and enums for the tool surface."""

from __future__ import annotations

from enum import Enum
from urllib.parse import parse_qs, urlencode, urlparse, urlunparse

from pydantic import BaseModel, ConfigDict, Field, field_validator

from .config import SETTINGS


class DownloadMode(str, Enum):
    """What to pull from the source."""

    AUDIO = "audio"
    VIDEO = "video"
    BOTH = "both"


class AudioFormat(str, Enum):
    """Target audio codec/container. 'best' avoids a re-encode when possible."""

    BEST = "best"
    MP3 = "mp3"
    M4A = "m4a"
    OPUS = "opus"
    FLAC = "flac"
    WAV = "wav"


class VideoContainer(str, Enum):
    """Container used when merging best video + best audio."""

    MP4 = "mp4"
    MKV = "mkv"


class ResponseFormat(str, Enum):
    """Output rendering for tool responses."""

    MARKDOWN = "markdown"
    JSON = "json"


_YT_HOSTS = {"youtube.com", "www.youtube.com", "m.youtube.com", "youtu.be"}
# Playlist ID prefixes that indicate a radio/mix rather than a real playlist.
_MIX_PREFIXES = ("RD", "RM", "WL")
# Query params that are only meaningful in the context of a mix/radio session.
_MIX_PARAMS = {"list", "start_radio", "index", "pp"}


def _strip_mix_params(url: str) -> str:
    """Return a clean YouTube video URL when the input is a mix/radio URL.

    YouTube mix URLs embed a real ``v=`` parameter but wrap it in a
    ``list=RD…`` auto-generated playlist.  yt-dlp resolves the playlist
    first, which starts with a different (often unavailable) video.
    Stripping the mix params lets yt-dlp go straight to the intended video.
    """
    try:
        parsed = urlparse(url)
    except Exception:
        return url
    if parsed.hostname not in _YT_HOSTS:
        return url
    qs = parse_qs(parsed.query, keep_blank_values=True)
    list_val = (qs.get("list") or [""])[0]
    if not list_val.startswith(_MIX_PREFIXES):
        return url
    # Keep only the video ID; drop all mix-session cruft.
    clean_qs = {k: v for k, v in qs.items() if k not in _MIX_PARAMS}
    return urlunparse(parsed._replace(query=urlencode(clean_qs, doseq=True)))


def _default_audio_format() -> AudioFormat:
    try:
        return AudioFormat(SETTINGS.audio_format)
    except ValueError:
        return AudioFormat.MP3


_DEFAULT_AUDIO_FORMAT = _default_audio_format()


class _UrlsMixin(BaseModel):
    """Shared URL field that also accepts a single bare string."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    urls: list[str] = Field(
        ...,
        description=(
            "One or more video URLs supported by yt-dlp (YouTube, Vimeo, etc.). "
            "A single URL string is accepted and coerced to a one-item list."
        ),
        min_length=1,
        max_length=100,
    )

    @field_validator("urls", mode="before")
    @classmethod
    def _coerce_single_url(cls, value: object) -> object:
        if isinstance(value, str):
            return [value]
        return value

    @field_validator("urls", mode="after")
    @classmethod
    def _normalize_urls(cls, value: list[str]) -> list[str]:
        return [_strip_mix_params(u) for u in value]


class DownloadInput(_UrlsMixin):
    """Input for ``youtube_download``."""

    mode: DownloadMode = Field(
        default=DownloadMode.AUDIO,
        description="'audio' (default), 'video', or 'both'.",
    )
    audio_format: AudioFormat = Field(
        default=_DEFAULT_AUDIO_FORMAT,
        description="Audio codec when mode includes audio. Default from YTDLP_AUDIO_FORMAT or 'mp3'.",
    )
    audio_quality: str = Field(
        default="0",
        description="yt-dlp audio quality for lossy codecs: '0' (best VBR) to '9', or a bitrate like '192K'. Ignored for best/flac/wav.",
        max_length=8,
    )
    max_height: int | None = Field(
        default=None,
        description="Cap video resolution by height, e.g. 1080 or 2160. None = best available.",
        ge=144,
        le=4320,
    )
    container: VideoContainer = Field(
        default=VideoContainer.MP4,
        description="Output container when downloading video.",
    )
    remote: str | None = Field(
        default=None,
        description="SSH remote to rsync to (an ~/.ssh/config alias or user@host). Falls back to YTDLP_REMOTE.",
        max_length=255,
    )
    dest_path: str | None = Field(
        default=None,
        description="Absolute destination directory on the remote for audio. Falls back to YTDLP_REMOTE_PATH.",
        max_length=1024,
    )
    video_dest_path: str | None = Field(
        default=None,
        description="Absolute destination directory on the remote for video. Falls back to YTDLP_VIDEO_REMOTE_PATH, then dest_path.",
        max_length=1024,
    )
    keep_local: bool = Field(
        default=False,
        description="Keep the local staging copy after a successful transfer instead of deleting it.",
    )
    use_archive: bool = Field(
        default=False,
        description=(
            "Record downloaded video IDs and skip them on future calls. Useful for "
            "repeat playlist/channel pulls. Audio and video are tracked separately. "
            "Archive location: YTDLP_ARCHIVE_DIR or ~/.local/state/youtube_dl_mcp."
        ),
    )
    response_format: ResponseFormat = Field(
        default=ResponseFormat.MARKDOWN,
        description="'markdown' (human-readable) or 'json' (machine-readable).",
    )


class ProbeInput(_UrlsMixin):
    """Input for ``youtube_probe``."""

    response_format: ResponseFormat = Field(
        default=ResponseFormat.MARKDOWN,
        description="'markdown' (human-readable) or 'json' (machine-readable).",
    )
