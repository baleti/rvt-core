"""robustness of the numpy-array kernels (hlr_raster, hlr_visible, feature_edges, smooth_part) with garbage input: NaN / inf / huge / negative / out-of-range indices / odd sizes.
Fine: a result, False/None (adapter declines) or a Python Exception. PANIC (BaseException), crash or hang = bug. usage: fuzz_kernels.py [N] [SEED]  (synthetic input, small memory)"""
import os, sys, random, time
import numpy as np
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_fast as RF
n = int(sys.argv[1]) if len(sys.argv) > 1 else 300; rnd = random.Random(int(sys.argv[2]) if len(sys.argv) > 2 else 1); rs = np.random.RandomState(rnd.randrange(1 << 30))
BAD = [np.nan, np.inf, -np.inf, 1e300, -1e300, 0.0, -1.0, 1e-300]
def arr(shape, sc=10.0):
    a = rs.uniform(-sc, sc, shape)
    for _ in range(rnd.choice([0, 0, 1, 5, 50])):
        if a.size: a.flat[rnd.randrange(a.size)] = rnd.choice(BAD)
    return a
stat = {}; panics = 0
def run(name, fn):
    global panics
    t = time.time()
    try: r = fn(); k = 'ok' if r is not None and r is not False else 'declined'
    except Exception: k = 'exc'
    except BaseException as e: k = 'PANIC'; panics += 1; print('PANIC', name, type(e).__name__, str(e)[:120], flush=True)
    if time.time() - t > 20: print('SLOW', name, '%.1fs' % (time.time() - t), flush=True)
    stat[(name, k)] = stat.get((name, k), 0) + 1
for i in range(n):
    m = rnd.choice([0, 1, 3, 40, 300]); W = rnd.choice([1, 2, 16, 64]); H = rnd.choice([1, 2, 16, 64])
    tris = arr((m, 3, 3)); zb = np.full((H, W), -1e30) if rnd.random() < 0.7 else arr((H, W))
    run('hlr_raster', lambda: RF.hlr_raster(tris, W, H, rnd.choice([0.0, -3.0, np.nan]), rnd.choice([0.0, 2.0, np.inf]), rnd.choice([0.1, 1.0, 0.0, -1.0, np.nan, 1e-300]), zb))
    segs = np.ascontiguousarray(arr((m, 2, 3))); ns = rnd.choice([1, 2, 5, 17]); out = np.zeros((m, ns), dtype=bool)
    run('hlr_visible', lambda: RF.hlr_visible(segs, ns, 0.0, 0.0, rnd.choice([0.1, 1.0, 0.0, np.nan]), zb, rnd.choice([1e-3, 0.0, np.nan, -1.0]), out))
    run('feature_edges', lambda: RF.feature_edges(tris, rnd.choice([0.5, 1.0, -1.0, 2.0, np.nan])))
    nv = rnd.choice([0, 1, 4, 50]); pv = arr((nv, 3)); pf = np.array([[rnd.choice([0, 1, 2, nv - 1, nv, -1, 10**6]) for _ in range(3)] for _ in range(rnd.choice([0, 1, 10, 80]))], dtype=np.int64).reshape(-1, 3)
    run('smooth_part', lambda: RF.smooth_part(pv, pf, rnd.choice([0.5, 1.0, -1.0, np.nan, 2.0])))
print(dict(sorted(stat.items())), 'panics', panics)
