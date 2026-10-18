#!/bin/sh
# if only rvt_params.py changed since the snapshot (Python-only post-processing, no port): diff_params on rst_basic 2020 (host3, ALLOW_HOST3, small); IDENTICAL -> accept the snapshot
cd "$(dirname "$0")/.."
ch=$(sh tests/oracle_watch.sh); [ -z "$ch" ] && { echo clean; exit 0; }
echo "$ch" | grep -v -E "^CHANGED (rvt_params|rvt_geom)\.py$" | grep -q . &&  { echo "other modules changed (port needed?): $ch"; exit 2; }
out=$(ALLOW_HOST3=1 PYTHONPATH=python python tests/diff_params.py $HOME/rvt-research/corpus/2020/18_rst_basic_sample_project.rvt 2>&1 | tail -1); echo "$out"
case "$out" in *IDENTICAL*) sh tests/oracle_watch.sh --accept >/dev/null; echo accepted;; *) exit 1;; esac
