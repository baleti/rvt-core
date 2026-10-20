#!/bin/sh
cd "$(dirname "$0")/.."; ./build.sh || exit 1
PY=$HOME/rvt-env/bin/python; C=$HOME/rvt-research/corpus; L=$HOME/h1logs/rvt-core/probe.log
RVT_FAST_DIR=$PWD/dev $PY tests/probe_tess.py $C/2020/18_rst_basic_sample_project.rvt 1046268 > $L 2>&1
RVT_FAST_DIR=$PWD/dev $PY tests/probe_tess.py $C/2020/20_Residential_Sample_Project.rvt 216328 >> $L 2>&1
echo finished >> $L
