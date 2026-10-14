# Nebulae Server — rtorrent daemon + headless Nebulae engine bridge.
#
# Build from the repository root:
#   docker build -f server/Dockerfile -t nebulae-server .
#
# Run:
#   docker run -d --name nebulae-server \
#     -p 9174:9174 -p 6881:6881 -p 6881:6881/udp \
#     -v /your/appdata/nebulae-server:/data \
#     -e NEBULAE_SERVER_TOKEN=change-me \
#     nebulae-server
#
# The desktop Nebulae app connects to http://<server>:9174 with that token.

# --- Stage 1: frontend (embedded by the Tauri build for the bin target) ----
# pnpm is installed from npm instead of corepack: corepack's bundled
# signature keys / default resolution fail in this stage, while the release
# workflow pins pnpm 12 via pnpm/action-setup with the same lockfile.
FROM node:22-slim AS frontend
WORKDIR /src
RUN npm install -g pnpm@12 --fetch-retries=5
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml* ./
RUN pnpm install --frozen-lockfile
COPY . .
RUN pnpm build

# --- Stage 2: compile the headless bridge binary ----------------------------
FROM rust:1-slim AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
        libwebkit2gtk-4.1-dev \
        libgtk-3-dev \
        build-essential \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY src-tauri ./src-tauri
COPY --from=frontend /src/dist ./dist
RUN cargo build --release --bin nebulae-server \
        --manifest-path src-tauri/Cargo.toml

# --- Stage 3: runtime with rtorrent -----------------------------------------
# Debian bookworm ships rtorrent 0.9.8 / libtorrent 0.13.8 (rakshasa) —
# a stable upstream release; the XML-RPC surface used by the bridge is
# unchanged from the versions we test against.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        rtorrent \
        dtach \
        ca-certificates \
        tini \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/src-tauri/target/release/nebulae-server /usr/local/bin/nebulae-server
COPY server/rtorrent.rc /root/.rtorrent.rc
COPY server/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh \
    && chmod +x /usr/local/bin/nebulae-server

# /data holds downloads, session state and the watch folder.
VOLUME ["/data"]
EXPOSE 9174 6881/tcp 6881/udp

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
