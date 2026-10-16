"""rvt_fast.feature_edges (Rust) vs rvtsheets.viewdraw._feature_edges(T, None) on real element triangles (random views, per element and merged) and synthetic meshes
(shared vertices, -0.0, duplicates, degenerate triangles): the returned (k,2,3) arrays must be bit-identical (same edges, same order, same rounded floats).
usage: diff_feature.py [--n N] FILE...   (host1: needs the sheets code's imports)"""
import os, sys, time, random, math
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/rvt-research')); sys.path.insert(0, os.path.expanduser('~/src/rvt-sheets-dev'))
import numpy as np, rvt_fast as RF
from rvtsheets import viewdraw as VD
n = 150
if '--n' in sys.argv: i = sys.argv.index('--n'); n = int(sys.argv[i + 1]); del sys.argv[i:i + 2]
cosmax = math.cos(VD.FEATURE_ANGLE); bad = 0
def same(T, tag):
    t = time.time(); a = VD._feature_edges(T, None); ta = time.time() - t
    t = time.time(); b = RF.feature_edges(T, cosmax); tb = time.time() - t
    ok = b is not None and a.shape == b.shape and a.tobytes() == b.tobytes()
    return ok, ta, tb, len(a)
# synthetic: a jittered grid with some -0.0, duplicated and degenerate triangles
rng = np.random.default_rng(2); G = rng.integers(0, 40, (3000, 3, 3)).astype(float) * 0.25 - 2.0
G[rng.random(G.shape) < 0.05] = -0.0; G[:50, 1] = G[:50, 0]; G = np.concatenate([G, G[:200]])
ok, ta, tb, k = same(G, 'synthetic'); print('synthetic %d triangles -> %d edges: %s' % (len(G), k, 'IDENTICAL' if ok else 'DIFFERENT'), flush=True); bad += not ok
for f in sys.argv[1:]:
    d = RF.Doc(f); ids = [e for e, i in d.res.items() if i.get('bbox')]; random.Random(4).shuffle(ids); nd = ne = 0; sa = sb = 0.0; allT = []
    for e in ids[:n]:
        try: m = RF.element_mesh_rust(d, e)
        except Exception: continue
        T = np.asarray(m.get('tris') or [], float).reshape(-1, 3, 3)
        if len(T): allT.append(T)
    sets = allT + [np.concatenate(allT[:40])] if allT else []
    for T in sets:
        ok, ta, tb, k = same(T, f); sa += ta; sb += tb; ne += k; nd += not ok
    bad += nd > 0
    print('%-40s %4d meshes %8d edges  python %6.2fs rust %6.2fs  %s' % (os.path.basename(f)[:40], len(sets), ne, sa, sb, 'IDENTICAL' if not nd else 'DIFFERENT %d' % nd), flush=True)
sys.exit(1 if bad else 0)
