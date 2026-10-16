"""rvt_fast.hlr_raster (Rust, parallel bands) vs rvtsheets.hlr._raster (numba) on real element triangles under random views: the depth buffers must be bit-identical.
usage: diff_raster.py [--n N] FILE...   (needs numba: host1 rvt env)"""
import os, sys, time, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/rvt-research')); sys.path.insert(0, os.path.expanduser('~/src/rvt-sheets-dev'))
import numpy as np, rvt_fast as RF
from rvtsheets import hlr
n = 400
if '--n' in sys.argv: i = sys.argv.index('--n'); n = int(sys.argv[i + 1]); del sys.argv[i:i + 2]
bad = 0
def synthetic():
    rng = np.random.default_rng(11); n = 30000
    base = rng.uniform(-50, 50, (n, 1, 3)); T = base + rng.normal(0, 1, (n, 3, 3)) * rng.choice([0.01, 0.3, 3, 20], (n, 1, 1))
    sl = rng.random(n) < 0.3; T[sl, 1] = T[sl, 0] + rng.uniform(-60, 60, (sl.sum(), 3)); T[sl, 2] = T[sl, 1] + rng.normal(0, 0.02, (sl.sum(), 3))
    return T
P = synthetic(); lo = P.reshape(-1, 3).min(0); hi = P.reshape(-1, 3).max(0); nd0 = 0
for px in (0.05, 0.2, 1.0):
    db = hlr.DepthBuffer(lo[0], lo[1], hi[0], hi[1], px)
    z1 = db.zb.copy(); hlr._raster(np.ascontiguousarray(P), db.W, db.H, db.u0, db.v0, db.px, z1)
    z2 = db.zb.copy(); RF.hlr_raster(P, db.W, db.H, db.u0, db.v0, db.px, z2)
    same = z1.tobytes() == z2.tobytes(); nd0 += not same
    print('synthetic 30000 triangles (slivers, steep) px %.2f buffer %dx%d %s' % (px, db.W, db.H, 'IDENTICAL' if same else 'DIFFERENT %d cells' % int((z1 != z2).sum())), flush=True)
bad += nd0 > 0
for f in sys.argv[1:]:
    d = RF.Doc(f); ids = [e for e, i in d.res.items() if i.get('bbox')]; random.Random(3).shuffle(ids)
    T = []
    for e in ids[:n]:
        try: m = RF.element_mesh_rust(d, e)
        except Exception: continue
        t = np.asarray(m.get('tris') or [], float).reshape(-1, 3, 3)
        if len(t): T.append(t)
    P = np.concatenate(T); print('%d triangles' % len(P), flush=True)
    rng = np.random.default_rng(5); nd = 0
    for view in range(4):
        a = rng.uniform(0, 6.28); b = rng.uniform(0.2, 1.4)
        R = np.array([[np.cos(a), -np.sin(a), 0], [np.sin(a), np.cos(a), 0], [0, 0, 1]]) @ np.array([[1, 0, 0], [0, np.cos(b), -np.sin(b)], [0, np.sin(b), np.cos(b)]])
        Q = (P.reshape(-1, 3) @ R.T).reshape(-1, 3, 3); lo = Q.reshape(-1, 3).min(0); hi = Q.reshape(-1, 3).max(0)
        px = max(hi[0] - lo[0], hi[1] - lo[1]) / 1800.0
        db = hlr.DepthBuffer(lo[0], lo[1], hi[0], hi[1], px)
        z1 = db.zb.copy(); t = time.time(); hlr._raster(np.ascontiguousarray(Q), db.W, db.H, db.u0, db.v0, db.px, z1); ta = time.time() - t
        z2 = db.zb.copy(); t = time.time(); ok = RF.hlr_raster(Q, db.W, db.H, db.u0, db.v0, db.px, z2); tb = time.time() - t
        same = ok and z1.tobytes() == z2.tobytes()
        nd += not same
        print('  view %d buffer %dx%d numba %.2fs rust %.2fs %s' % (view, db.W, db.H, ta, tb, 'IDENTICAL' if same else 'DIFFERENT (%d cells)' % int((z1 != z2).sum()) if ok else 'rust off'), flush=True)
    bad += nd > 0
    print('%-40s %s' % (os.path.basename(f)[:40], 'IDENTICAL' if not nd else 'DIFFERENT %d' % nd), flush=True)
sys.exit(1 if bad else 0)
