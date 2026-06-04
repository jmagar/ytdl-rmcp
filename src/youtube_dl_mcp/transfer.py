"""Transfer staged files to an SSH remote with rsync.

Passwordless key-based auth is assumed; ``-o BatchMode=yes`` (set in
:data:`config.DEFAULT_SSH_OPTS`) guarantees we fail fast rather than blocking on
an interactive password prompt.
"""

from __future__ import annotations

import shlex
import subprocess
from collections.abc import Sequence
from pathlib import Path

MKDIR_TIMEOUT = 60  # seconds
RSYNC_TIMEOUT = 60 * 60 * 6  # 6 hours, for very large transfers


class TransferError(RuntimeError):
    """Raised when remote dir creation or rsync fails."""


def _ssh_e(ssh_opts: Sequence[str]) -> str:
    return "ssh " + " ".join(shlex.quote(opt) for opt in ssh_opts)


def ensure_remote_dir(remote: str, dest_path: str, ssh_opts: Sequence[str]) -> None:
    """Create the destination directory tree on the remote (idempotent)."""

    cmd = ["ssh", *ssh_opts, remote, "mkdir", "-p", "--", dest_path]
    proc = subprocess.run(
        cmd, capture_output=True, text=True, timeout=MKDIR_TIMEOUT, check=False
    )
    if proc.returncode != 0:
        detail = proc.stderr.strip() or proc.stdout.strip() or "no output"
        raise TransferError(
            f"Could not create '{dest_path}' on '{remote}': {detail}. "
            "Check the remote alias, your SSH key, and write permissions."
        )


def rsync(
    staging_dir: Path,
    remote: str,
    dest_path: str,
    ssh_opts: Sequence[str],
) -> str:
    """Sync the contents of ``staging_dir`` into ``dest_path`` on ``remote``.

    The trailing slash on the source tells rsync to copy the *contents*
    (including any artist subdirectories) rather than the staging dir itself.
    Destination subdirectories are created automatically.
    """
    target = f"{remote}:{shlex.quote(dest_path)}/"
    cmd = [
        "rsync",
        "-av",
        "--partial",
        "--human-readable",
        "-e",
        _ssh_e(ssh_opts),
        f"{staging_dir}/",
        target,
    ]
    proc = subprocess.run(
        cmd, capture_output=True, text=True, timeout=RSYNC_TIMEOUT, check=False
    )
    if proc.returncode != 0:
        detail = proc.stderr.strip() or proc.stdout.strip() or "no output"
        raise TransferError(f"rsync failed (exit {proc.returncode}): {detail}")
    return proc.stdout.strip()
