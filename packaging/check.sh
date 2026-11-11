#!/bin/sh
set -eu
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node --test tests/placement.mjs
python3 -m unittest discover -s tests -p 'test_*.py'
find packaging -name '*.sh' -exec sh -n {} \;
sh -n packaging/appimage/AppRun
cargo run --locked -- --once
