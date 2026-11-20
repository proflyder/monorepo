ARG SOURCE_DATE_EPOCH=0

FROM debian:trixie-20261020@sha256:4c25275446ca371766b8b093c696782eabce06263fccacb8d5d813b3dc44b308 AS sqlite_build
ARG DEBIAN_SNAPSHOT=20261020T000000Z
ARG SQLITE_AUTOCONF_VERSION=3530400
ARG SQLITE_SHA256=0e9483900e92cd5de8fd48d16bf9200145a61f7fd5be542a5ac81d8a9516eb9c
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    set -eu; \
    rm -f /etc/apt/sources.list /etc/apt/sources.list.d/*.sources; \
    printf '%s\n' \
        'Types: deb' \
        "URIs: http://snapshot.debian.org/archive/debian/${DEBIAN_SNAPSHOT}" \
        'Suites: trixie trixie-updates' \
        'Components: main' \
        'Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp' \
        '' \
        'Types: deb' \
        "URIs: http://snapshot.debian.org/archive/debian-security/${DEBIAN_SNAPSHOT}" \
        'Suites: trixie-security' \
        'Components: main' \
        'Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp' \
        > /etc/apt/sources.list.d/debian.snapshot.sources; \
    apt_opts="-o Acquire::Retries=8 -o Acquire::http::Timeout=30 \
        -o Acquire::https::Timeout=30 -o Acquire::Check-Valid-Until=false"; \
    install_packages() { \
        for attempt in 1 2 3; do \
            if apt-get $apt_opts install -y --no-install-recommends "$@"; then \
                return 0; \
            fi; \
            echo "apt-get install failed (attempt ${attempt}/3); retrying" >&2; \
            sleep $((attempt * 5)); \
            apt-get $apt_opts update || true; \
        done; \
        apt-get $apt_opts install -y --fix-missing --no-install-recommends "$@"; \
    }; \
    apt-get $apt_opts update; \
    install_packages build-essential ca-certificates curl; \
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


FROM ghcr.io/astral-sh/uv:0.12.3@sha256:2d890623d310b57771ce840f0da5eed5fc6d657da05ffaa45d82797b53fa3abc AS uv
FROM python:3.14.9-slim-trixie@sha256:35caba83af2fe6da9a5d922f9b0156f21d1246a48278c58a08bd253303ba42e8
ARG DEBIAN_SNAPSHOT=20261020T000000Z
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    set -eu; \
    rm -f /etc/apt/sources.list /etc/apt/sources.list.d/*.sources; \
    printf '%s\n' \
        'Types: deb' \
        "URIs: http://snapshot.debian.org/archive/debian/${DEBIAN_SNAPSHOT}" \
        'Suites: trixie trixie-updates' \
        'Components: main' \
        'Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp' \
        '' \
        'Types: deb' \
        "URIs: http://snapshot.debian.org/archive/debian-security/${DEBIAN_SNAPSHOT}" \
        'Suites: trixie-security' \
        'Components: main' \
        'Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp' \
        > /etc/apt/sources.list.d/debian.snapshot.sources; \
    apt_opts="-o Acquire::Retries=8 -o Acquire::http::Timeout=30 \
        -o Acquire::https::Timeout=30 -o Acquire::Check-Valid-Until=false"; \
    install_packages() { \
        for attempt in 1 2 3; do \
            if apt-get $apt_opts install -y --no-install-recommends "$@"; then \
                return 0; \
            fi; \
            echo "apt-get install failed (attempt ${attempt}/3); retrying" >&2; \
            sleep $((attempt * 5)); \
            apt-get $apt_opts update || true; \
        done; \
        apt-get $apt_opts install -y --fix-missing --no-install-recommends "$@"; \
    }; \
    apt-get $apt_opts update; \
    install_packages ca-certificates curl git ffmpeg libatomic1 tini tzdata; \
    rm -rf /var/log/apt /var/log/dpkg.log /var/log/alternatives.log \
        /var/cache/fontconfig /var/cache/ldconfig
COPY --from=uv /uv /usr/local/bin/uv
COPY --from=sqlite_build /opt/sqlite-fixed/lib/libsqlite3.so.3.53.4 /usr/local/lib/
RUN ln -sf libsqlite3.so.3.53.4 /usr/local/lib/libsqlite3.so.0 && ldconfig && \
    rm -rf /var/cache/ldconfig && \
    python3 -c "import sqlite3; assert sqlite3.sqlite_version_info >= (3, 51, 3)"
WORKDIR /opt/hermes
COPY . /opt/hermes
ENV UV_PROJECT_ENVIRONMENT=/opt/venv UV_PYTHON_DOWNLOADS=never PYTHONUNBUFFERED=1 \
    UV_COMPILE_BYTECODE=0 PYTHONDONTWRITEBYTECODE=1
RUN --mount=type=cache,target=/root/.cache/uv,sharing=locked \
    uv sync --frozen --no-dev --python /usr/local/bin/python3 --extra mcp --extra web --extra messaging && \
    find /opt/venv -name uv_cache.json -delete && \
    find /opt/venv -name RECORD -exec sed -i '/uv_cache.json/d' {} +
ENV PATH=/opt/venv/bin:$PATH HERMES_HOME=/opt/data HOME=/opt/data
WORKDIR /opt/data
LABEL org.opencontainers.image.revision="bf0203a794a07aac3ff764972ab9087baa4e4f4e"
ENTRYPOINT ["/usr/bin/tini", "--", "/opt/venv/bin/hermes"]
CMD ["gateway", "run"]
