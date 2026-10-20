#!/bin/sh
cd "$(dirname "$0")/.."; ./build.sh || exit 1
PY=$HOME/rvt-env/bin/python; C=$HOME/rvt-research/corpus; L=$HOME/h1logs/rvt-core/probe.log; : > $L
for v in "RVT_NOSPLITCOL=1" "RVT_NOHOLEREFINE=1" "RVT_NOSPLITCOL=1 RVT_NOHOLEREFINE=1"; do
  echo "### $v" >> $L
  env $v RVT_FAST_DIR=$PWD/dev $PY tests/probe_tess.py $C/2020/18_rst_basic_sample_project.rvt 1046268 2>&1 | grep -c "face " >> $L
done
echo finished >> $L
