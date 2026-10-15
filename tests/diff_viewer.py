"""rvt_viewer_fast vs the viewer's own reader path (rvtviewer.reader.load with the reader as it is today, _em_geometry on a sample). usage: diff_viewer.py FILE [N]"""
import os, sys, time, random
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
sys.path.insert(0, os.path.expanduser('~/src/rvt-viewer'))
import rvt_viewer_fast as VF
from canon import digest
f = sys.argv[1]; N = int(sys.argv[2]) if len(sys.argv) > 2 else 200
from rvtviewer import reader, build
t = time.time(); a = reader.load(f, log=lambda *x: None); ta = time.time() - t
m = VF.open(f); t = time.time(); b = VF.load(f, log=lambda *x: None, model=m); tb = time.time() - t
ok = all(digest(a[k]) == digest(b[k]) for k in ('eh', 'W', 'params', 'classes', 'tagbase')) and list(a['recs']) == list(b['recs']) and all(digest(a['recs'][k]) == digest(b['recs'][k]) for k in a['recs'])
print('load: viewer %.1fs fast %.1fs %s' % (ta, tb, 'IDENTICAL' if ok else 'DIFFERENT'))
import rvt_geometry as GG
ids = m.mesh_ids(); random.Random(2).shuffle(ids); ids = ids[:N]
same = diff = 0; t = time.time()
for e in ids:
    x = build._em_geometry(GG, f, e); y = VF.em_geometry(m, e)
    def norm_(z):
        z = dict(z); mr = z.pop('meta_raw', None); tr = z.pop('tris_raw', None)
        import numpy as np
        ev = None
        if mr is not None:
            ev = [(k, None if f_ is None or uv is None else [np.asarray(f_(float(u), float(v)), float).tolist() for u, v in uv], None if uv is None else [list(map(float, q)) for q in uv]) for k, f_, uv in mr]
        return z, ev, None if tr is None else [np.asarray(p, float).tolist() for t_ in tr for p in t_]
    if norm_(x) == norm_(y): same += 1
    else:
        diff += 1
        if diff <= 3: print('  differs', e, {k: (x.get(k) == y.get(k)) for k in set(x) | set(y)})
print('em_geometry: %d identical, %d differ (%.1fs)' % (same, diff, time.time() - t))
