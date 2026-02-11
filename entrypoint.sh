#!/bin/sh
# ensure the data and music directories are writable by the swingmusic user.
# docker volumes/bind-mounts can override image-layer ownership, so we fix
# permissions at runtime before dropping to the unprivileged user.

if [ "$(id -u)" = "0" ]; then
    chown -R swingmusic:swingmusic /data /music 2>/dev/null || true
    exec su-exec swingmusic "$@"
else
    exec "$@"
fi
