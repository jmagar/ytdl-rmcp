"""Keep yt-dlp fresh.

yt-dlp's YouTube extractor rots quickly, so we check the installed version's
date and ``pip install -U`` it when it's older than a threshold. This MUST run
before yt-dlp is first imported in the process: a pip update can't hot-swap a
module that's already loaded, so :func:`ensure_fresh` is intended to be called
at startup, with yt-dlp imported lazily everywhere else.
"""

from __future__ import annotations

import subprocess
import sys
import time
from datetime import date
from importlib import metadata

_PIP_TIMEOUT = 300  # seconds
_CHECK_INTERVAL = 6 * 3600  # don't re-check more often than this at runtime
_last_check: float = 0.0


def _installed_version() -> str | None:
    try:
        return metadata.version("yt-dlp")
    except metadata.PackageNotFoundError:
        return None


def _age_days(version: str) -> int | None:
    """Parse a date-based yt-dlp version (YYYY.M.D[.build]) into an age in days."""
    parts = version.split(".")
    if len(parts) < 3:
        return None
    try:
        released = date(int(parts[0]), int(parts[1]), int(parts[2]))
    except ValueError:
        return None
    return (date.today() - released).days


def is_stale(max_age_days: int) -> tuple[bool, str | None, int | None]:
    version = _installed_version()
    if version is None:
        return False, None, None
    age = _age_days(version)
    if age is None:
        return False, version, None
    return age > max_age_days, version, age


def _pip_update(pre: bool) -> tuple[bool, str]:
    cmd = [sys.executable, "-m", "pip", "install", "-U"]
    cmd += ["--pre", "yt-dlp[default]"] if pre else ["yt-dlp"]
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=_PIP_TIMEOUT, check=False
        )
    except subprocess.TimeoutExpired:
        return False, "pip update timed out"
    if proc.returncode != 0:
        detail = (proc.stderr.strip() or proc.stdout.strip() or "pip failed")[-300:]
        return False, detail
    return True, _installed_version() or "unknown"


def ensure_fresh(
    max_age_days: int, pre: bool, *, respect_interval: bool = True
) -> dict:
    """Update yt-dlp if it's older than ``max_age_days``.

    Returns a small status dict describing what happened: one of
    ``absent`` | ``current`` | ``updated`` | ``failed`` | ``skipped``.
    Set ``respect_interval=False`` to force a check (e.g. at startup).
    """
    global _last_check
    now = time.time()
    if respect_interval and (now - _last_check) < _CHECK_INTERVAL:
        return {"action": "skipped", "reason": "checked recently"}
    _last_check = now

    stale, version, age = is_stale(max_age_days)
    if version is None:
        return {"action": "absent"}
    if not stale:
        return {"action": "current", "version": version, "age_days": age}

    ok, detail = _pip_update(pre)
    if ok:
        return {"action": "updated", "from": version, "to": detail}
    return {"action": "failed", "version": version, "age_days": age, "error": detail}
