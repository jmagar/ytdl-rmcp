# syntax=docker/dockerfile:1

# Base images pinned by digest (cd-m3). Re-resolve with:
#   docker buildx imagetools inspect <image:tag>   (top-level "Digest:")
FROM rust:1-bookworm@sha256:19817ead3289c8c631c73df281e18b59b172f6a31f4f563290f69cddd06c30e9 AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY assets ./assets

RUN cargo build --release --locked

FROM debian:bookworm-slim@sha256:96e378d7e6531ac9a15ad505478fcc2e69f371b10f5cdf87857c4b8188404716 AS runtime

# OCI image labels (cd-m3). The CI workflow's docker/metadata-action also emits
# org.opencontainers.image.* labels (revision, created, etc.) and passes them to
# build-push-action; these static defaults cover local/manual builds.
ARG VCS_REF=unknown
LABEL org.opencontainers.image.title="ytdl-rmcp" \
      org.opencontainers.image.description="Cross-platform single-binary MCP server: downloads media with yt-dlp, embeds metadata + cover art, organizes by artist, and transfers to local, SSH, or rclone targets." \
      org.opencontainers.image.source="https://github.com/jmagar/ytdl-rmcp" \
      org.opencontainers.image.url="https://github.com/jmagar/ytdl-rmcp" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.revision="${VCS_REF}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        ffmpeg \
        libchromaprint-tools \
        openssh-client \
        rclone \
        rsync \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /usr/sbin/nologin --uid 10001 ytdl

COPY --from=builder /app/target/release/rytdl /usr/local/bin/ytdl-rmcp

# Use the ffmpeg baked in via apt instead of auto-downloading at runtime (cd-m3):
# removes the second ffmpeg provenance path and a runtime network dependency.
# yt-dlp is intentionally NOT installed in the image; it bootstraps at runtime.
ENV HOME=/home/ytdl \
    YTDLP_STAGING_DIR=/tmp/ytdl-rmcp \
    FPCALC_PATH=/usr/bin/fpcalc \
    FFMPEG_PATH=/usr/bin/ffmpeg

RUN mkdir -p /tmp/ytdl-rmcp /home/ytdl/.local/state/ytdl-rmcp /home/ytdl/.cache \
    && chown -R ytdl:ytdl /tmp/ytdl-rmcp /home/ytdl

USER ytdl
WORKDIR /work

VOLUME ["/library", "/home/ytdl/.ssh", "/home/ytdl/.local/state/ytdl-rmcp", "/home/ytdl/.cache"]

# Liveness check: the binary answers --version fast and exits 0 without starting
# the stdio server, so it never interferes with the MCP transport (cd-m3).
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["ytdl-rmcp", "--version"]

ENTRYPOINT ["ytdl-rmcp"]
CMD ["serve"]
