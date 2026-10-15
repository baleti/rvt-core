#!/bin/bash
# run the rvt-core differential suites on host1 politely (shared 9 GB slice): one job at a time, 2 GB per file, start only while the slice uses < 4.5 GB, and re-check between jobs.
# usage: tests/h1_polite.sh   (runs in the foreground on host3; the jobs themselves run on host1)
slice_ok() { cur=$(ssh host1 'systemctl --user show rvt-offload.slice -p MemoryCurrent --value' 2>/dev/null); [ -n "$cur" ] && [ "$cur" -lt 4500000000 ]; }
run_job() {  # name, env assignments..., suites...
  local name=$1; shift
  while ! slice_ok; do sleep 120; done
  ~/.local/share/offload/h1run run $name --env rvt --dir ~/src/rvt-core -- env FILE_MEM=2G "$@" 2>&1 | tail -1
  sleep 20
  while [ "$(ssh host1 "systemctl --user is-active h1-$name.service" 2>/dev/null)" = active ]; do sleep 120; done
  echo "$name finished $(date +%H:%M)"
}
run_job rvtcore-x EXTRA=1 tests/h1_suite.sh extract_all g103 params mesh refvol
run_job rvtcore-r2 tests/h1_suite.sh params mesh refvol extract_all
