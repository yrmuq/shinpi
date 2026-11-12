#!/bin/sh
set -eu
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
node --test tests/placement.mjs
find packaging -name '*.sh' -exec sh -n {} \;
sh -n packaging/appimage/AppRun
cargo run --locked -- --once
