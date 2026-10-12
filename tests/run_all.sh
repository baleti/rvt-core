#!/bin/sh
# corpus-wide differential tests against the Python oracle, one file at a time, on a snapshot of the current build.
# usage: tests/run_all.sh [suite...]   suites: extract_all extract_default extract_slim g103 (default: all). Logs: logs/<suite>.log
cd "$(dirname "$0")/.."
SNAP=$(mktemp -d "${TMPDIR:-/tmp}/rvtfast.XXXXXX"); cp python/rvt_fast.py python/rvt_core.abi3.so "$SNAP"/
export RVT_FAST_DIR="$SNAP"
PY=${PY:-$HOME/src/rvt-viewer/.venv/bin/python}
FILES=$(ls -Sr $HOME/rvt-research/corpus/*/*)
SUITES=${*:-"extract_all g103 extract_default extract_slim"}
for s in $SUITES; do
  case $s in
    extract_all) $PY tests/diff_extract.py --want-all $FILES > logs/$s.log 2>&1 ;;
    extract_default) $PY tests/diff_extract.py --default $FILES > logs/$s.log 2>&1 ;;
    extract_slim) $PY tests/diff_extract.py --slim $FILES > logs/$s.log 2>&1 ;;
    g103) $PY tests/diff_g103.py $FILES > logs/$s.log 2>&1 ;;
  esac
  echo "$s exit $?" >> logs/summary.log
done
