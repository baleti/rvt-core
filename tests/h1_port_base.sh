#!/bin/sh
# baseline: same diff_mesh with the RELEASED build (python/) for comparison with portchk.log
cd "$(dirname "$0")/.."
PY=$HOME/rvt-env/bin/python; C=$HOME/rvt-research/corpus; L=$HOME/h1logs/rvt-core/portbase.log
for f in $C/2026/58_Snowdon_Towers_Sample_Facades.rvt $C/2020/18_rst_basic_sample_project.rvt; do
  echo "== $f" >> $L
  systemd-run --user --scope -q -p MemoryMax=5G -p MemorySwapMax=0 $PY tests/diff_mesh.py --impl rust --n 600 $f >> $L 2>&1
done
echo finished >> $L
