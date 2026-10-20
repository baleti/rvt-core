"""probe a diff_mesh difference: tessellate every Geometry node of element EID (and its symbols) with the reference and Rust, per differing node list the faces whose triangles differ
(face kind, ring count, tri counts). usage: probe_tess.py FILE EID..."""
import os, sys, collections
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import numpy as np, rvt_fast as RF, rvt_elemmesh as EM, rvt_mesh as RM
RC = RF._rc(); d = RF.Doc(sys.argv[1])
for e in map(int, sys.argv[2:]):
    rec = d.rec(e); geos, inst = EM._geos_f(rec['fields'], 3); syms = set()
    for z in inst or []:
        try: syms.add(int(EM._ob(z['m_instanceInfo']).get('m_symbolId')))
        except Exception: pass
    allg = list(geos)
    for s in syms:
        r2 = d.rec(s)
        if r2 is not None:
            try: allg += EM._geos_f(r2['fields'], 3)[0]
            except Exception: pass
    print('element', e, 'nodes', len(allg))
    for gi, g in enumerate(allg):
        a = RM.tessellate_geometry(g); b = RC.tessellate_geometry(g)
        ta = np.asarray(a['tris'], float).reshape(-1, 3, 3); tb = np.asarray(b['tris'], float).reshape(-1, 3, 3)
        if ta.shape == tb.shape and ta.tobytes() == tb.tobytes(): continue
        fa = collections.defaultdict(list); fb = collections.defaultdict(list)
        for k, f in enumerate(a['fidx']): fa[f].append(ta[k].tobytes())
        for k, f in enumerate(b['fidx']): fb[f].append(tb[k].tobytes())
        print(' node', gi, 'tris', len(ta), len(tb))
        for f in sorted(set(fa) | set(fb)):
            if fa.get(f) != fb.get(f):
                m = a['meta'][[k for k, x in enumerate(a['fidx']) if x == f][0]] if f in fa else None
                print('   face', f, 'ref', len(fa.get(f, [])), 'rust', len(fb.get(f, [])), 'sorted-equal', sorted(fa.get(f, [])) == sorted(fb.get(f, [])), 'kind', (m or ('?',))[0] if m else None)

        if os.environ.get('PROBE_DUMP'):
            f = int(os.environ['PROBE_DUMP']); A = [ta[k] for k, x in enumerate(a['fidx']) if x == f]; B = [tb[k] for k, x in enumerate(b['fidx']) if x == f]
            sa = {x.tobytes() for x in A}
            for k, (x, y) in enumerate(zip(A, B)):
                if x.tobytes() != y.tobytes(): print('   tri', k, 'ref', np.round(x, 6).tolist(), 'rust', np.round(y, 6).tolist()); break
            print('   in-ref-not-rust', len([y for y in B if y.tobytes() not in sa]), 'of', len(B))
