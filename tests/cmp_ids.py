"""element_mesh reference (Python on the fast Doc) vs Rust for given ids: python tests/cmp_ids.py FILE ID..."""
import os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import rvt_fast as RF, numpy as np
d = RF.Doc(sys.argv[1]); bad = 0
for e in map(int, sys.argv[2:]):
    t = time.time(); a = RF.element_mesh(d, e); ta = time.time() - t; t = time.time(); b = RF.element_mesh_rust(d, e); tb = time.time() - t
    same = all(a.get(k) == b.get(k) for k in ('fid', 'nid', 'flags', 'source', 'mat')) and len(a['tris']) == len(b['tris']) and (not a['tris'] or (np.array(a['tris'], float) == np.array(b['tris'], float)).all())
    bad += not same
    print(e, len(a['tris']), len(b['tris']), a.get('flags'), 'IDENTICAL' if same else 'DIFFERENT', 'reference %.1fs rust %.2fs' % (ta, tb), flush=True)
sys.exit(1 if bad else 0)
