"""robustness of rvt_fast.refined_volume (Rust) on real element_mesh results with mutated triangles / fid / levels / tol. Fine: result or Python Exception; PANIC / crash / hang = bug.
usage: fuzz_refvol.py FILE [N_ELEMENTS] [MUTATIONS] [SEED]"""
import os, sys, random, copy, time
import numpy as np
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_fast as RF
f = sys.argv[1]; ne = int(sys.argv[2]) if len(sys.argv) > 2 else 40; mut = int(sys.argv[3]) if len(sys.argv) > 3 else 20; rnd = random.Random(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
d = RF.Doc(f); d._census(); ids = sorted(e for e, i in d.res.items() if i.get('bbox') and d._cen['cls'].get(e) and not d._cen['cls'][e].endswith('Symbol')); rnd.shuffle(ids)
BAD = [float('nan'), float('inf'), -float('inf'), 1e300, -1e300, 0.0, -1.0, 1e-300]
meshes = []
for e in ids:
    try: m = RF.element_mesh_rust(d, e)
    except Exception: continue
    if m.get('tris') and 0 < len(m['tris']) <= 1500: meshes.append(m)
    if len(meshes) >= ne: break
ok = exc = panics = 0; t0 = time.time()
for m in meshes:
    for _ in range(mut):
        tris = [[list(map(float, p)) for p in t] for t in m['tris']]; fid = list(m['fid']) if m.get('fid') is not None else None; meta = list(m['meta'])
        for _ in range(rnd.choice([0, 1, 3, 20])):
            r = rnd.random(); i = rnd.randrange(len(tris))
            if r < 0.7: tris[i][rnd.randrange(3)][rnd.randrange(3)] = rnd.choice(BAD)
            elif r < 0.8 and fid: fid[i] = rnd.choice([-1, 0, 10**9, 2**40])
            elif r < 0.9: meta[i] = (meta[i][0], None, meta[i][2])
            else: tris.pop(i); (fid and fid.pop(i)); meta.pop(i)
            if not tris: break
        if not tris: continue
        if fid is not None and len(fid) != len(tris): fid = None
        t = time.time()
        try: RF.refined_volume(tris, meta[:len(tris)], levels=rnd.choice([0, 1, 3, 5]), tol=rnd.choice([5e-5, 0.0, 1.0, float('nan')]), centroid=rnd.random() < 0.3, fid=fid); ok += 1
        except Exception: exc += 1
        except BaseException as ex: panics += 1; print('PANIC', type(ex).__name__, str(ex)[:140], flush=True)
        if time.time() - t > 30: print('SLOW %.1fs' % (time.time() - t), flush=True)
print('elements', len(meshes), 'ok', ok, 'exc', exc, 'panics', panics, '%.0fs' % (time.time() - t0))
