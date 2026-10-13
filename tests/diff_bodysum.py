"""Differential test: rvt_viewer_fast.body_summary (Rust) vs the viewer's rvtviewer.build.body_summary on the same em_geometries meshes. usage: diff_bodysum.py FILE [N]"""
import os, sys, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/src/rvt-viewer'))
import numpy as np, rvt_viewer_fast as VF
from rvtviewer import build
f = sys.argv[1]; N = int(sys.argv[2]) if len(sys.argv) > 2 else 400
m = VF.open(f, progress=False); m.quick(); ids = m.mesh_ids(); random.Random(3).shuffle(ids); ids = ids[:N]
bad = n = 0
for e, g in VF.em_geometries(m, ids):
    if not g.get('solids'): continue
    P = np.array(g['solids'][0]['triangles'], float).reshape(-1, 3)
    a = build.body_summary(P, g['nid'], g.get('fid'), g.get('surf')); b = VF.body_summary(P, g['nid'], g.get('fid'), g.get('surf'))
    n += 1
    if a != b: bad += 1; print('  DIFF', e, [(x, y) for x, y in zip(a, b) if x != y][:1]) if bad <= 3 else None
print('%-44s %4d meshes  %s' % (os.path.basename(f)[:44], n, 'IDENTICAL' if not bad else 'DIFFERENT %d' % bad)); sys.exit(1 if bad else 0)
