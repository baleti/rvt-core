#!/bin/sh
# after every release: the viewer's demo smoke (tools/demo_smoke.sh) on the three demo files of ~/src/rvt-viewer/DEMO.md, capped and niced
C=$HOME/rvt-research/corpus; ok=0
for f in 2020/20_Residential_Sample_Project.rvt 2024/43_Snowdon_Towers_Sample_Structural.rvt 2026/58_Snowdon_Towers_Sample_Facades.rvt; do
  r=$(cd $HOME/src/rvt-viewer && systemd-run --user --scope -q -p MemoryMax=6G -p MemorySwapMax=0 -p CPUQuota=400% nice -n 19 sh tools/demo_smoke.sh $C/$f 8992 2>&1 | tail -1)
  echo "$f: $r"; [ "$r" = "DEMO-SMOKE OK" ] || ok=1
done
exit $ok
