#!/bin/sh
# which oracle modules changed since they were last ported/tested (snapshot in logs/snap); --accept copies the current versions into the snapshot
cd "$(dirname "$0")/.."
R=$HOME/rvt-research
for f in rvt_mesh.py rvt_elemmesh.py rvt_curves.py rvt_entities.py rvt_decode.py g103_get.py rvt_es.py rvt_scan.py rvt_params.py rvt_map2.py rvt_ifc.py rvt_famparams.py rvt_geom.py rvt_uid.py ifcguid.py fmt_parser.py rvtfmt.py rvt2text.py rvt_inventory.py rvt_materials.py; do
  [ -f "$R/$f" ] || continue
  if ! cmp -s "$R/$f" "logs/snap/$f"; then echo "CHANGED $f"; [ "$1" = "--accept" ] && cp -p "$R/$f" "logs/snap/$f"; fi
done
