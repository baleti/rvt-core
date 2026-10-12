#!/bin/bash
# corpus differential suites for remote-host (h1run). usage: tests/h1_suite.sh SUITE...  suites: extract_all_rest extract_default extract_slim g103 params mesh
# logs: logs/h1_<suite>.log ; uses a snapshot of the released build; oracle side single worker (memory-light)
cd "$(dirname "$0")/.."
SNAP=$(mktemp -d /tmp/rvtfast.XXXXXX); cp python/rvt_fast.py python/rvt_core.abi3.so "$SNAP"/
export RVT_FAST_DIR="$SNAP" RVT_CORE_NOCACHE=1 RVT_WORKERS=1 RVT_THREADS=4
PY=${PY:-python}
ALL=$(ls -Sr $HOME/rvt-research/corpus/*/*)
SUB=$(ls -Sr $HOME/rvt-research/corpus/*/* | awk 'NR % 3 == 1')
for s in "$@"; do
  case $s in
    extract_all_rest) REST=$(for f in $ALL; do grep -q "^$(basename $f | cut -c1-50) " logs/extract_all.remote-host.log || echo $f; done); $PY tests/diff_extract.py --want-all $REST ;;
    extract_default) $PY tests/diff_extract.py --default $SUB ;;
    extract_slim) $PY tests/diff_extract.py --slim $SUB ;;
    g103) $PY tests/diff_g103.py $ALL ;;
    params) $PY tests/diff_params.py $ALL ;;
    mesh) $PY tests/diff_mesh.py --impl rust --n 300 $SUB ;;
  esac > logs/h1_$s.log 2>&1
  echo "$s exit $? $(date +%H:%M)" >> logs/h1_summary.log
done
