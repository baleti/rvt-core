#!/bin/bash
# after tests/h1_big2.sh: element meshes + refined_volume over the WHOLE main corpus (until now one file in three)
cd "$(dirname "$0")/.."
while pgrep -f tests/h1_big2.sh >/dev/null; do sleep 120; done
eval "$(sed -n '/^slice_ok/p;/^run_job() {/,/^}/p' tests/h1_big.sh)"
run_job rvtcore-b5 tests/h1_suite.sh mesh_all refvol_all
