#!/bin/bash
# robustness sweep (stream) on main + extra, then 2018 rme_advanced (05) extract/params/slim/mesh/refvol at 6G (reference OOM at 4G)
cd "$(dirname "$0")/.."
eval "$(sed -n '/^slice_ok/p;/^run_job() {/,/^}/p' tests/h1_big.sh)"
run_job rvtcore-c1 tests/h1_suite.sh stream
run_job rvtcore-c2 EXTRA=1 tests/h1_suite.sh stream
ssh host1 'cd ~/h1logs/rvt-core && for f in h1_extract_all.log h1_params.log h1_extract_slim.log h1_mesh.log h1_refvol.log; do sed -i "/^05_rme_advanced.*FAILED rc=/d" $f; done'
run_job rvtcore-c3 env FILE_MEM=6G tests/h1_suite.sh extract_all params extract_slim mesh_all refvol_all
