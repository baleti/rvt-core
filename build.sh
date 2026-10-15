#!/bin/sh
# development build (bounded memory) -> dev/ (rvt_fast.py + rvt_core.abi3.so). python/ is the RELEASED copy that session 176's hooks import: only release.sh writes it. Python sources: pysrc/.
set -e
cd "$(dirname "$0")"
systemd-run --user --scope -q -p MemoryMax=6G -p MemorySwapMax=0 cargo build --release -j4 2>&1 | grep -E "^(warning|error)" -A5 | grep -v "never used\|never constructed\|never read" || true
mkdir -p dev
cp target/release/librvt_core.so dev/rvt_core.abi3.so.tmp && mv dev/rvt_core.abi3.so.tmp dev/rvt_core.abi3.so
cp pysrc/rvt_fast.py pysrc/rvt_viewer_fast.py dev/
