#!/bin/sh
# wait (max ~6 h) until h1run accepts, then run the Nanjing probe on host1; output: logs/nanjing_probe.out
cd "$(dirname "$0")/.."
F=$HOME/rvt-research/samples/2011_1_111122_029522_Nanjing__YOC_MEP_SD2-UPDATE.rvt
i=0
while [ $i -lt 72 ]; do
  out=$(~/.local/share/offload/h1run run nanjprobe --env rvt --dir "$PWD" -- python3 bench/nanjing_probe.py "$F" 546245 2>&1)
  case "$out" in *busy*) i=$((i+1)); sleep 300;; *) echo "$out"; exit 0;; esac
done
echo gave up
