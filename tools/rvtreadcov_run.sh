#!/bin/bash
# usage: rvtreadcov_run.sh OUT.jsonl [--records] FILE...   one file at a time, 6G scope, nice
out=$1; shift; flag=""; [ "$1" = "--records" ] && { flag=--records; shift; }
for f in "$@"; do
  systemd-run --user --scope -q -p MemoryMax=6G nice -n 10 timeout 3600 python3 "$(dirname "$0")/rvtreadcov.py" "$f" $flag >> "$out" 2>>"$out.err" || echo "{\"open\":\"error:runner rc=$?\"}" >> "$out"
done
