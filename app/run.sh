#!/usr/bin/env bash
# custom-protocol is required or Tauri serves devUrl and the window fails
# with "connection refused".
set -e
cd "$(dirname "$0")"
npm run build --silent
cd src-tauri
cargo build --release --bin chaptr-app --features custom-protocol
# chaptr finds its tools in bin/ and its models in models/, next to the binary.
# On Windows the installer puts them there; in a dev checkout they are symlinks.
exec ./target/release/chaptr-app "$@"
