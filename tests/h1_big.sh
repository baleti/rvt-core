#!/bin/bash
# retry the files that did not fit the 2 GB cap: 4 GB per file, start only while the shared remote-host slice uses < 2.5 GB (leaves >= 2.5 GB for others), one job at a time. Logs are resumable (files with results are skipped).
slice_ok() { cur=$(ssh remote-host 'systemctl --user show rvt-offload.slice -p MemoryCurrent --value' 2>/dev/null); [ -n "$cur" ] && [ "$cur" -lt 2500000000 ]; }
run_job() {
  local name=$1; shift
  while ! slice_ok; do sleep 300; done
  ~/.local/share/offload/h1run run $name --env rvt --dir ~/src/rvt-core -- env FILE_MEM=4G "$@" 2>&1 | tail -1
  sleep 20
  while [ "$(ssh remote-host "systemctl --user is-active h1-$name.service" 2>/dev/null)" = active ]; do sleep 120; done
  echo "$name finished $(date +%H:%M)"
}
run_job rvtcore-b1 tests/h1_suite.sh extract_all params
run_job rvtcore-b2 EXTRA=1 tests/h1_suite.sh extract_all params mesh refvol
run_job rvtcore-b3 tests/h1_suite.sh mesh refvol
