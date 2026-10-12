#!/bin/sh
# build the extension (bounded memory) and place it in python/ (tests and adapters import it from there; deploy.sh copies it to ~/rvt-research)
set -e
cd "$(dirname "$0")"
systemd-run --user --scope -q -p MemoryMax=6G -p MemorySwapMax=0 cargo build --release -j4 2>&1 | grep -E "^(warning|error)" -A5 | grep -v "never used\|never constructed" || true
cp target/release/librvt_core.so python/rvt_core.abi3.so.tmp && mv python/rvt_core.abi3.so.tmp python/rvt_core.abi3.so
