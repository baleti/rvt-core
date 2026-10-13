#!/bin/bash
# corpus differential suites for remote-host (h1run). usage: tests/h1_suite.sh SUITE...  suites: extract_all extract_default extract_slim g103 params mesh refvol; FILE_MEM=8G raises the per-file cap
# logs: $HOME/h1logs/rvt-core/h1_<suite>.log (outside the h1run-synced tree: any agent's `h1run sync` would overwrite them; pull with h1run pull h1logs/rvt-core) (appended, resumable: files that already have a result line are skipped); one python process per file inside its own
# scope (MemoryMax 6G, same slice) so one heavy file or a neighbour's memory cannot kill the whole suite; a killed file is logged as "FAILED rc".
# uses a snapshot of the released build; oracle side single worker (memory-light)
cd "$(dirname "$0")/.."
SNAP=$(mktemp -d /tmp/rvtfast.XXXXXX); cp python/rvt_fast.py python/rvt_core.abi3.so "$SNAP"/
export RVT_FAST_DIR="$SNAP" RVT_CORE_NOCACHE=1 RVT_WORKERS=1 RVT_THREADS=4
PY=${PY:-python}
L=$HOME/h1logs/rvt-core; mkdir -p $L
ALL=$(ls -Sr $HOME/rvt-research/corpus/*/*)
SUB=$(ls -Sr $HOME/rvt-research/corpus/*/* | awk 'NR % 3 == 1')
run1() {  # suite log file args...
  local log=$1 f=$2; shift 2
  grep -q "^$(basename "$f" | cut -c1-40)" "$log" 2>/dev/null && return 0
  systemd-run --user --scope --quiet --slice=rvt-offload.slice -p MemoryMax=${FILE_MEM:-6G} -p MemorySwapMax=0 $PY "$@" "$f" >> "$log" 2>> "$log.err"
  local rc=$?; [ $rc -gt 1 ] && printf '%-48s FAILED rc=%d\n' "$(basename "$f" | cut -c1-48)" $rc >> "$log"
  return 0
}
for s in "$@"; do
  log=$L/h1_$s.log
  case $s in
    extract_all) for f in $ALL; do run1 $log $f tests/diff_extract.py --want-all; done ;;
    extract_default) for f in $SUB; do run1 $log $f tests/diff_extract.py --default; done ;;
    extract_slim) for f in $SUB; do run1 $log $f tests/diff_extract.py --slim; done ;;
    g103) for f in $ALL; do run1 $log $f tests/diff_g103.py; done ;;
    params) for f in $ALL; do run1 $log $f tests/diff_params.py; done ;;
    mesh) for f in $SUB; do run1 $log $f tests/diff_mesh.py --impl rust --n 300; done ;;
    refvol) for f in $SUB; do run1 $log $f tests/diff_refvol.py; done ;;
  esac
  ok=$(grep -cE 'IDENTICAL|  OK$' $log); fl=$(grep -cE 'FAILED rc|^FAIL ' $log); n=$(grep -cvE '^  ' $log)
  echo "$s done $(date +%H:%M): $ok ok, $fl failed, $((n - ok - fl)) DIFFERENT" >> $L/h1_summary.log
done
