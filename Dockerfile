FROM rust:1.85-bookworm AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    libsqlite3-dev \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

# ---------------------------------------------------------------------------
# runtime image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    ffmpeg \
    libsqlite3-0 \
    libssl3 \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# create the unprivileged user and the two volume mount-points up front so
# ownership is baked into the image regardless of whether volumes are mounted
RUN groupadd -r swingmusic \
    && useradd -r -g swingmusic -d /data -s /usr/sbin/nologin swingmusic \
    && mkdir -p /data /music \
    && chown -R swingmusic:swingmusic /data /music

COPY --from=builder /app/target/release/swingmusic /usr/local/bin/swingmusic
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENV HOME=/data

# admin credentials (defaults to admin:admin when empty)
ENV SWING_ADMIN_USERNAME=""
ENV SWING_ADMIN_PASSWORD=""

# music root directories - this is the canonical way to tell the container
# where your music lives. when a volume is mounted at /music the default
# value below means it just works out of the box with zero extra config.
# multiple roots can be colon-separated, e.g. /music:/podcasts
ENV SWING_ROOT_DIRS="/music"

# declare volumes so docker knows these are external mount-points.
# /music  -> bind-mount your music library here
# /data   -> persistent config, database, thumbnails, etc.
VOLUME ["/music", "/data"]

EXPOSE 1970

# start as root so the entrypoint can fix volume permissions,
# then drop to the swingmusic user via su-exec
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["swingmusic", "--host", "0.0.0.0", "--port", "1970", "--config", "/data"]
