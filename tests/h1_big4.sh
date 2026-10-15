#!/bin/bash
# after tests/h1_big3.sh: robustness sweep of the whole fast pipeline (tests/stream_all.py) over the main corpus and the extra samples
cd "$(dirname "$0")/.."
while pgrep -f tests/h1_big3.sh >/dev/null; do sleep 120; done
eval "$(sed -n '/^slice_ok/p;/^run_job/,/^}/p' tests/h1_big.sh)"
run_job rvtcore-b6 tests/h1_suite.sh stream
run_job rvtcore-b7 EXTRA=1 tests/h1_suite.sh stream
