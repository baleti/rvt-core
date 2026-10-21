"""robustness of the Rust MeshDoc entry points: random element ids (valid, invalid, 0, u64 max) with extreme symbol_depth / detail. A Python Exception is fine; PANIC or crash is a bug.
usage: fuzz_meshdoc.py FILE [N] [SEED]"""
import os, sys, random, time
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_fast as RF
f = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 500; rnd = random.Random(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
d = RF.Doc(f); md = RF.mesh_doc(d); ids = list(d.res)
panics = exc = ok = 0
for _ in range(n):
    e = rnd.choice([rnd.choice(ids), rnd.randrange(0, 2**32), 0, 2**64 - 1, rnd.choice(ids) + 1])
    dp = rnd.choice([-5, -1, 0, 1, 2, 3, 9, 100, 2**31 - 1, -2**31]); dt = rnd.choice([-1, 0, 1, 2, 3, 4, 5, 99, 2**40, -2**40])
    try:
        if rnd.random() < .5: md.element_mesh(e, dp, dt)
        else: md.element_meshes([e, rnd.choice(ids)], dp, dt, rnd.random() < .5)
        ok += 1
    except OverflowError: exc += 1
    except Exception: exc += 1
    except BaseException as ex: panics += 1; print('PANIC', e, dp, dt, type(ex).__name__, str(ex)[:150], flush=True)
print('ok %d exc %d PANICS %d' % (ok, exc, panics))
