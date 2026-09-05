#!/usr/bin/env bash
# custom-protocol is required or Tauri serves devUrl and the window fails
# with "connection refused".
set -e
cd "$(dirname "$0")"
npm run build --silent
cd src-tauri
cargo build --release --bin chaptr-app --features custom-protocol
# WebKitGTK's DMA-BUF renderer dies on this compositor with
# "Error 71 (Protocol error) dispatching to Wayland display".
export WEBKIT_DISABLE_DMABUF_RENDERER=1
exec ./target/release/chaptr-app "$@"
