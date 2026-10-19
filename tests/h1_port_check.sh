#!/bin/sh
# host1: build dev/ then diff_mesh + diff_tess on files with curved faces. usage (via h1run --env rust): tests/h1_port_check.sh [N]
cd "$(dirname "$0")/.."
N=${1:-600}; PY=$HOME/rvt-env/bin/python; C=$HOME/rvt-research/corpus; L=$HOME/h1logs/rvt-core/portchk.log
./build.sh || exit 1
for f in $C/2020/18_rst_basic_sample_project.rvt $C/2020/20_Residential_Sample_Project.rvt $C/2026/58_Snowdon_Towers_Sample_Facades.rvt $C/2024/46_Snowdon_Towers_Sample_Facades.rvt $C/2024/47_Snowdon_Towers_Sample_Site.rvt; do
  echo "== $f" >> $L
  RVT_FAST_DIR=$PWD/dev systemd-run --user --scope -q -p MemoryMax=5G -p MemorySwapMax=0 $PY tests/diff_mesh.py --impl rust --n $N $f >> $L 2>&1
done
echo finished >> $L
