#!/bin/bash
# after tests/h1_big3.sh: robustness sweep of the whole fast pipeline (tests/stream_all.py) over the main corpus and the extra samples
cd "$(dirname "$0")/.."
while pgrep -f tests/h1_big3.sh >/dev/null; do sleep 120; done
eval "$(sed -n '/^slice_ok/p;/^run_job() {/,/^}/p' tests/h1_big.sh)"
run_job rvtcore-b6 tests/h1_suite.sh stream
run_job rvtcore-b7 EXTRA=1 tests/h1_suite.sh stream
# last: the 2011 Nanjing sample (reference OOM at 4G for mesh/refvol): its FAILED lines removed, rerun alone at 6G (others are skipped by the resumable logs)
ssh host1 'cd ~/h1logs/rvt-core && sed -i "/Nanjing.*FAILED rc=/d" h1_x_mesh.log; sed -i "/Nanjing/d" h1_x_refvol.log'
run_job rvtcore-b8 env FILE_MEM=6G EXTRA=1 tests/h1_suite.sh mesh refvol
