#!/bin/sh
# README timings: oracle (pure Python) and fast modes per file, one measurement at a time. Output: logs/bench.jsonl
cd "$(dirname "$0")/.."
PY=${PY:-$HOME/src/rvt-viewer/.venv/bin/python}
C=$HOME/rvt-research/corpus
for f in $C/2022/30_rst_basic_sample_project.rvt $C/2024/43_Snowdon_Towers_Sample_Structural.rvt $C/2024/44_Snowdon_Towers_Sample_Electrical.rvt $C/2025/52_Snowdon_Towers_Sample_Facades.rvt; do
  for m in fast-cold fast-new fast-warm oracle; do
    systemd-run --user --scope -q -p MemoryMax=6G -p MemorySwapMax=0 $PY bench/pipeline.py $m $f --sample 200 >> logs/bench.jsonl 2>> logs/bench.err
  done
done
