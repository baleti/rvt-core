#!/bin/bash
# after tests/h1_big.sh: retry the last memory-failed main-corpus files (2018 rme_advanced extract/params, 2021 rme_advanced slim)
cd "$(dirname "$0")/.."
while pgrep -f tests/h1_big.sh >/dev/null; do sleep 120; done
eval "$(sed -n '/^slice_ok/p;/^run_job() {/,/^}/p' tests/h1_big.sh)"
run_job rvtcore-b4 tests/h1_suite.sh extract_all params extract_slim
