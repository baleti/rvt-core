#!/bin/sh
# digest of rvt_fast.extract(want_all) over the whole corpus + extra samples for the released build (python/) and the dev build (dev/): any differing digest is printed (host1)
cd "$(dirname "$0")/.."
PY=${PY:-$HOME/rvt-env/bin/python}
FILES=$(ls -S $HOME/rvt-research/corpus/*/* $HOME/rvt-research/samples/* $HOME/rvt-research/samples-rfa/* 2>/dev/null)
$PY bench/xdigest.py $FILES 2>&1 | sed 's/ released$//' | awk '{print $1, $4, $6, $NF}' > /tmp/cmpb_rel.txt
RVT_FAST_DIR=$PWD/dev $PY bench/xdigest.py $FILES 2>&1 | sed 's#/home/user1/src/rvt-core/dev$##' | awk '{print $1, $4, $6, $NF}' > /tmp/cmpb_dev.txt
wc -l /tmp/cmpb_rel.txt /tmp/cmpb_dev.txt
diff /tmp/cmpb_rel.txt /tmp/cmpb_dev.txt && echo "ALL IDENTICAL"
