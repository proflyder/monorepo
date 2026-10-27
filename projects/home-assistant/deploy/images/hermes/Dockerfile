# Debian 13 still ships SQLite 3.46.1, which contains the upstream WAL-reset
# corruption bug. Build a pinned shared library for the runtime image instead
# of relying on a distro backport that trixie does not currently provide.
# See #70480 and https://sqlite.org/wal.html#walresetbug.
# Pinned by the multi-arch index digest: uv and node already come from pm's
# sha-verified lock, and a tag alone would let the base drift under them.

FROM debian:trixie-20261020@sha256:4c25275446ca371766b8b093c696782eabce06263fccacb8d5d813b3dc44b308 AS sqlite_build
ARG SQLITE_AUTOCONF_VERSION=3530400
ARG SQLITE_SHA256=0e9483900e92cd5de8fd48d16bf9200145a61f7fd5be542a5ac81d8a9516eb9c
RUN apt-get -o Acquire::Retries=3 update && \
    apt-get -o Acquire::Retries=3 install -y --no-install-recommends \
        build-essential ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    (curl -fsSL --retry 1 --retry-all-errors --connect-timeout 15 --max-time 60 \
        -o /tmp/sqlite.tar.gz \
        "https://sqlite.org/2026/sqlite-autoconf-${SQLITE_AUTOCONF_VERSION}.tar.gz" || \
     curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 120 \
        -o /tmp/sqlite.tar.gz \
        "https://sources.buildroot.net/sqlite/sqlite-autoconf-${SQLITE_AUTOCONF_VERSION}.tar.gz") && \
    printf '%s  %s\n' "${SQLITE_SHA256}" /tmp/sqlite.tar.gz > /tmp/sqlite.sha256 && \
    sha256sum -c /tmp/sqlite.sha256 && \
    tar -xzf /tmp/sqlite.tar.gz -C /tmp && \
    cd "/tmp/sqlite-autoconf-${SQLITE_AUTOCONF_VERSION}" && \
    CFLAGS="-O2 \
        -DSQLITE_ENABLE_FTS3 \
        -DSQLITE_ENABLE_FTS3_PARENTHESIS \
        -DSQLITE_ENABLE_FTS4 \
        -DSQLITE_ENABLE_FTS5 \
        -DSQLITE_ENABLE_RTREE \
        -DSQLITE_ENABLE_GEOPOLY \
        -DSQLITE_ENABLE_COLUMN_METADATA \
        -DSQLITE_ENABLE_UNLOCK_NOTIFY \
        -DSQLITE_ENABLE_DBSTAT_VTAB \
        -DSQLITE_ENABLE_DBPAGE_VTAB \
        -DSQLITE_ENABLE_MATH_FUNCTIONS \
        -DSQLITE_ENABLE_PREUPDATE_HOOK \
        -DSQLITE_ENABLE_SESSION \
        -DSQLITE_SECURE_DELETE \
        -DSQLITE_THREADSAFE=1 \
        -DSQLITE_MAX_VARIABLE_NUMBER=250000" \
        ./configure --prefix=/opt/sqlite-fixed --disable-static && \
    make -j"$(nproc)" && \
    make install


FROM ghcr.io/astral-sh/uv:0.12.3 AS uv
FROM python:3.14.9-slim-trixie
COPY --from=uv /uv /usr/local/bin/uv
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git ffmpeg libatomic1 tini && rm -rf /var/lib/apt/lists/*
COPY --from=sqlite_build /opt/sqlite-fixed/lib/libsqlite3.so.3.53.4 /usr/local/lib/
RUN ln -sf libsqlite3.so.3.53.4 /usr/local/lib/libsqlite3.so.0 && ldconfig && python3 -c "import sqlite3; assert sqlite3.sqlite_version_info >= (3, 51, 3)"
WORKDIR /opt/hermes
# Build context is a git archive of the pinned, reviewed Hermes revision, never HOME.
COPY . /opt/hermes
ENV UV_PROJECT_ENVIRONMENT=/opt/venv UV_PYTHON_DOWNLOADS=never PYTHONUNBUFFERED=1
RUN uv sync --frozen --no-dev --python /usr/local/bin/python3 --extra mcp --extra web --extra messaging
ENV PATH=/opt/venv/bin:$PATH HERMES_HOME=/opt/data HOME=/opt/data
WORKDIR /opt/data
LABEL org.opencontainers.image.revision="bf0203a794a07aac3ff764972ab9087baa4e4f4e"
ENTRYPOINT ["/usr/bin/tini", "--", "/opt/venv/bin/hermes"]
CMD ["gateway", "run"]
