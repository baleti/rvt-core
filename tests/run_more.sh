#!/bin/sh
# second test queue (after rvtcore-tests): ES (all), params (all), g103 + mesh + extract default/slim on every 3rd file. Logs: logs/<suite>.log
cd "$(dirname "$0")/.."
while systemctl --user is-active -q rvtcore-tests; do sleep 60; done
SNAP=$(mktemp -d "${TMPDIR:-/tmp}/rvtfast.XXXXXX"); cp python/rvt_fast.py python/rvt_core.abi3.so "$SNAP"/
export RVT_FAST_DIR="$SNAP" RVT_CORE_NOCACHE=1
PY=${PY:-$HOME/src/rvt-viewer/.venv/bin/python}
ALL=$(ls -Sr $HOME/rvt-research/corpus/*/*)
SUB=$(ls -Sr $HOME/rvt-research/corpus/*/* | awk 'NR % 3 == 1')
$PY tests/diff_es.py $ALL > logs/es.log 2>&1; echo "es exit $?" >> logs/summary.log
$PY tests/diff_params.py $ALL > logs/params.log 2>&1; echo "params exit $?" >> logs/summary.log
$PY tests/diff_mesh.py --impl rust --n 300 $SUB > logs/mesh.log 2>&1; echo "mesh exit $?" >> logs/summary.log
$PY tests/diff_g103.py $SUB > logs/g103_sub.log 2>&1; echo "g103_sub exit $?" >> logs/summary.log
$PY tests/diff_extract.py --default $SUB > logs/extract_default.log 2>&1; echo "extract_default exit $?" >> logs/summary.log
$PY tests/diff_extract.py --slim $SUB > logs/extract_slim.log 2>&1; echo "extract_slim exit $?" >> logs/summary.log
