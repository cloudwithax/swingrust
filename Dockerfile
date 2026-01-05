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

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r swingmusic \
    && useradd -r -g swingmusic -d /data -s /usr/sbin/nologin swingmusic \
    && mkdir -p /data \
    && chown -R swingmusic:swingmusic /data

COPY --from=builder /app/target/release/swingmusic /usr/local/bin/swingmusic

ENV HOME=/data

# optional: set admin credentials via environment variables
# defaults to admin:admin if not specified
ENV SWING_ADMIN_USERNAME=""
ENV SWING_ADMIN_PASSWORD=""

EXPOSE 1970

USER swingmusic

ENTRYPOINT ["swingmusic"]
CMD ["--host", "0.0.0.0", "--port", "1970", "--config", "/data"]
