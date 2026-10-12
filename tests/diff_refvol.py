"""Differential test: rvt_core.refined_volume (Rust) vs rvt_mesh.refined_volume (reference) on the same element_mesh results (Rust meshes, whose evaluators
both sides can call). usage: diff_refvol.py FILE [N]   relative tolerance 1e-9 on volume and area; exit 1 on any difference"""
import os, sys, time, random
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import rvt_fast as RF, rvt_mesh as M
f = sys.argv[1]; N = int(sys.argv[2]) if len(sys.argv) > 2 else 300
d = RF.Doc(f); md = RF.mesh_doc(d); RC = RF._rc()
ids = [e for e in RF.Model(f, progress=False).mesh_ids()]; random.Random(7).shuffle(ids); ids = ids[:N]
bad = n = 0; ta = tb = 0
batch = md.md.refined_volumes(ids)
for e, bt in zip(ids, batch):
    m = md.element_mesh(e)
    if not m['tris'] or len(m.get('meta') or []) != len(m['tris']): continue
    t = time.time()
    try: a = M.refined_volume(m['tris'], m['meta'], fid=m.get('fid'))
    except Exception: a = None
    ta += time.time() - t; t = time.time()
    try: b = RC.refined_volume([[list(map(float, p)) for p in tr] for tr in m['tris']], m['meta'], fid=m.get('fid'))
    except ValueError: b = None
    tb += time.time() - t; n += 1
    ok = (a is None and b is None) or (a is not None and b is not None and a[1] == b[1] and abs(a[0] - b[0]) <= 1e-9 * max(1, abs(a[0])) and abs(a[2] - b[2]) <= 1e-9 * max(1, abs(a[2])))
    ok = ok and (bt is None) == (b is None) and (b is None or (abs(bt[0] - b[0]) <= 1e-12 * max(1, abs(b[0])) and abs(bt[2] - b[2]) <= 1e-12 * max(1, abs(b[2]))))
    if not ok: bad += 1; print('  DIFF', e, a, b, bt) if bad <= 5 else None
print('%-44s %4d meshes  reference %6.2fs rust %5.2fs  %s' % (os.path.basename(f)[:44], n, ta, tb, 'IDENTICAL(1e-9)' if not bad else 'DIFFERENT %d' % bad), flush=True)
sys.exit(1 if bad else 0)
