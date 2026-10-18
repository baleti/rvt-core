"""robustness of rvt_core.tessellate_geometry: deep-copy decoded Geometry nodes, mutate random numbers (NaN, inf, huge, 0, negative, swapped ints) and drop/replace random dict keys, call the Rust entry.
Fine: a result or a Python Exception. PANIC (pyo3 PanicException, a BaseException) or a crash/hang is a bug. usage: fuzz_tess.py FILE [N_NODES] [MUTATIONS_PER_NODE] [SEED]   (host1: loads one file)"""
import os, sys, random, copy, time, math
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_fast as RF, rvt_elemmesh as EM
f = sys.argv[1]; nn = int(sys.argv[2]) if len(sys.argv) > 2 else 100; mut = int(sys.argv[3]) if len(sys.argv) > 3 else 20; rnd = random.Random(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
RC = RF._rc(); d = RF.Doc(f); ids = [e for e, i in d.res.items() if i.get('bbox')]; rnd.shuffle(ids)
nodes = []
for e in ids:
    rec = d.rec(e)
    if rec is None: continue
    try: g, _ = EM._geos_f(rec['fields'], 3)
    except Exception: continue
    nodes += g
    if len(nodes) >= nn: break
BAD = [float('nan'), float('inf'), -float('inf'), 1e300, -1e300, 0.0, -1.0, 1e-300, 4294967295, -1, 2**40]
def slots(x, path=()):
    if isinstance(x, dict):
        for k, v in x.items():
            yield (x, k); yield from slots(v, path + (k,))
    elif isinstance(x, list):
        for i, v in enumerate(x):
            yield (x, i); yield from slots(v, path + (i,))
panics = exc = ok = 0; t0 = time.time()
for g0 in nodes:
    for _ in range(mut):
        g = copy.deepcopy(g0); ss = list(slots(g))
        for _ in range(rnd.choice([1, 3, 10])):
            c, k = rnd.choice(ss)
            try:
                r = rnd.random()
                if r < 0.6: c[k] = rnd.choice(BAD)
                elif r < 0.8 and isinstance(c, dict): del c[k]
                elif r < 0.9 and isinstance(c, list): del c[k]
                else: c[k] = None
            except Exception: pass
        t = time.time()
        try: RC.tessellate_geometry(g); ok += 1
        except Exception: exc += 1
        except BaseException as e: panics += 1; print('PANIC', type(e).__name__, str(e)[:160], flush=True)
        if time.time() - t > 20: print('SLOW %.1fs' % (time.time() - t), flush=True)
print('nodes', len(nodes), 'ok', ok, 'exc', exc, 'panics', panics, '%.0fs' % (time.time() - t0))
