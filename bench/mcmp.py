"""quick Rust-vs-Python element_mesh comparison on the fast Doc: python bench/mcmp.py FILE N"""
import sys, time, os
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'python')); import rvt_fast as RF
import numpy as np
f = sys.argv[1]; N = int(sys.argv[2])
d = RF.Doc(f); cen = d._census()
ids = sorted(e for e, i in d.res.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol'))
import random; random.Random(7).shuffle(ids); ids = ids[:N]
ta = tb = 0; bad = 0; tol_bad = 0; ex = []
for e in ids:
    t = time.time()
    try: a = RF.element_mesh(d, e)
    except Exception: a = None
    ta += time.time() - t; t = time.time()
    try: b = RF.element_mesh_rust(d, e)
    except Exception: b = None
    tb += time.time() - t
    if (a is None) != (b is None): bad += 1; ex.append((e, 'exc', a is None)); continue
    if a is None: continue
    same = len(a['tris']) == len(b['tris']) and a['flags'] == b['flags'] and a['source'] == b['source'] and a.get('fid') == b.get('fid') and a.get('nid') == b.get('nid') and a.get('mat') == b.get('mat')
    if same and a['tris']:
        A = np.array([p for t_ in a['tris'] for p in t_]); B = np.array([p for t_ in b['tris'] for p in t_])
        dmax = float(np.abs(A - B).max())
        if dmax > 0: tol_bad += 1
        if dmax > 1e-9: same = False
    if not same:
        bad += 1; ex.append((e, cen['cls'][e], (len(a['tris']), a['flags'], a['source']), (len(b['tris']), b['flags'], b['source'])))
print('elements', len(ids), 'different', bad, 'not bit-identical but <=1e-9', tol_bad, 'python %.1fs rust %.1fs' % (ta, tb))
for x in ex[:8]: print(' ', x)
