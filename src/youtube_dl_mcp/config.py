"""Runtime configuration, sourced from environment variables.

All values are optional and can be overridden per tool call (where applicable).
They exist so the server can be wired up once (in the MCP client config) without
repeating settings on every request.
"""

from __future__ import annotations

import os
from dataclasses import dataclass

# Fail fast instead of hanging on a password prompt: key-based auth is assumed.
DEFAULT_SSH_OPTS: tuple[str, ...] = ("-o", "BatchMode=yes")

# Where the download archive lives when enabled but no dir is configured.
DEFAULT_ARCHIVE_DIR = os.path.join(
    os.path.expanduser("~"), ".local", "state", "youtube_dl_mcp"
)


def _as_bool(value: str | None, default: bool) -> bool:
    if value is None:
        return default
    return value.strip().lower() in ("1", "true", "yes", "on")


def _as_int(value: str | None, default: int) -> int:
    try:
        return int(value) if value is not None else default
    except ValueError:
        return default


@dataclass(frozen=True, slots=True)
class Settings:
    """Server defaults resolved from the process environment."""

    remote: str | None
    dest_path: str | None
    video_dest_path: str | None
    staging_dir: str | None
    audio_format: str
    ssh_opts: tuple[str, ...]
    archive_dir: str
    auto_update: bool
    max_age_days: int
    update_pre: bool

    @classmethod
    def from_env(cls) -> "Settings":
        extra_opts = tuple(os.environ.get("YTDLP_SSH_OPTS", "").split())
        return cls(
            remote=os.environ.get("YTDLP_REMOTE") or None,
            dest_path=os.environ.get("YTDLP_REMOTE_PATH") or None,
            video_dest_path=os.environ.get("YTDLP_VIDEO_REMOTE_PATH") or None,
            staging_dir=os.environ.get("YTDLP_STAGING_DIR") or None,
            audio_format=os.environ.get("YTDLP_AUDIO_FORMAT", "mp3"),
            ssh_opts=DEFAULT_SSH_OPTS + extra_opts,
            archive_dir=os.environ.get("YTDLP_ARCHIVE_DIR") or DEFAULT_ARCHIVE_DIR,
            auto_update=_as_bool(os.environ.get("YTDLP_AUTO_UPDATE"), True),
            max_age_days=_as_int(os.environ.get("YTDLP_MAX_AGE_DAYS"), 90),
            update_pre=_as_bool(os.environ.get("YTDLP_UPDATE_PRE"), False),
        )


SETTINGS = Settings.from_env()
