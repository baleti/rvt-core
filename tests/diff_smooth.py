"""rvt_fast.smooth_part (Rust) vs rvtviewer.build.smooth_part (numpy) on real element meshes and synthetic meshes: positions, faces, normals must be bit-identical.
usage: diff_smooth.py [--n N] FILE...   (host1, rvt env)"""
import os, sys, random, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/rvt-research')); sys.path.insert(0, os.path.expanduser('~/src/rvt-viewer'))
import numpy as np, rvt_fast as RF
from rvtviewer import build as B
n = 200
if '--n' in sys.argv: i = sys.argv.index('--n'); n = int(sys.argv[i + 1]); del sys.argv[i:i + 2]
bad = 0
def same(pv, pf, tag):
    t = time.time(); a = B.smooth_part(pv, pf); ta = time.time() - t
    t = time.time(); b = RF.smooth_part(pv, pf, B.SMOOTH_COS); tb = time.time() - t
    ok = b is not None and all(x.shape == y.shape and x.tobytes() == y.tobytes() for x, y in zip(a, b))
    return ok, ta, tb
rng = np.random.default_rng(3); pv = rng.integers(0, 30, (4000, 3)).astype(float) * 0.37; pf = rng.integers(0, 4000, (3000, 3))
ok, ta, tb = same(pv, pf, 'synthetic'); print('synthetic: %s' % ('IDENTICAL' if ok else 'DIFFERENT'), flush=True); bad += not ok
for f in sys.argv[1:]:
    d = RF.Doc(f); ids = [e for e, i in d.res.items() if i.get('bbox')]; random.Random(5).shuffle(ids); nd = k = 0; sa = sb = 0.0
    for e in ids[:n]:
        try: m = RF.element_mesh_rust(d, e)
        except Exception: continue
        T = np.asarray(m.get('tris') or [], float).reshape(-1, 3)
        if len(T) == 0: continue
        ok, ta, tb = same(T, np.arange(len(T)).reshape(-1, 3), f); sa += ta; sb += tb; k += 1; nd += not ok
    bad += nd > 0
    print('%-40s %4d meshes python %6.2fs rust %6.2fs %s' % (os.path.basename(f)[:40], k, sa, sb, 'IDENTICAL' if not nd else 'DIFFERENT %d' % nd), flush=True)
sys.exit(1 if bad else 0)
