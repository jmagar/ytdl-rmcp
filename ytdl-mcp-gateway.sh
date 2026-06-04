#!/usr/bin/env bash
export YTDLP_REMOTE="tootie"
export YTDLP_REMOTE_PATH="/mnt/user/data/media/music/yt-dlp"
export YTDLP_VIDEO_REMOTE_PATH="/mnt/user/data/media/movies/yt-dlp"
export YTDLP_AUTO_UPDATE="0"
exec "$(dirname "$0")/.venv/bin/youtube-dl-mcp" "$@"
