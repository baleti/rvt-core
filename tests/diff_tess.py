"""Differential test: rvt_core.tessellate_geometry(geo dict) (Rust, Python-dict entry used by rvt-sheets) vs rvt_mesh.tessellate_geometry on the same decoded Geometry nodes
(own nodes of a seeded sample of placed elements + the nodes of the symbols they instantiate). Compares triangles bit-exactly, fidx, frs, fprio, unsupported, faces; both raising counts as equal.
usage: diff_tess.py [--n N] FILE...   exit 1 on any difference"""
import os, sys, random, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import numpy as np, rvt_fast as RF, rvt_elemmesh as EM, rvt_mesh as RM
n = 150
if '--n' in sys.argv: i = sys.argv.index('--n'); n = int(sys.argv[i + 1]); del sys.argv[i:i + 2]
RC = RF._rc(); bad = 0
for f in sys.argv[1:]:
    d = RF.Doc(f); ids = [e for e, i in d.res.items() if i.get('bbox')]
    random.Random(1).shuffle(ids); nodes = 0; nd = 0; tr = tx = 0.0; err = 0
    syms = set()
    for e in ids[:n]:
        rec = d.rec(e)
        if rec is None: continue
        try: geos, inst = EM._geos_f(rec['fields'], 3)
        except Exception: continue
        if not geos:
            try: geos, inst = EM._geos(rec['fields'])
            except Exception: continue
        for z in inst or []:
            try: s = EM._ob(z['m_instanceInfo']).get('m_symbolId')
            except Exception: s = None
            if s is not None: syms.add(int(s))
        allg = list(geos)
        for s in list(syms)[:5]:
            r2 = d.rec(s)
            if r2 is not None:
                try: allg += EM._geos_f(r2['fields'], 3)[0]
                except Exception: pass
        for g in allg:
            nodes += 1
            t = time.time()
            try: a = RM.tessellate_geometry(g); ea = None
            except Exception as ex: a = None; ea = ex
            tr += time.time() - t; t = time.time()
            try: b = RC.tessellate_geometry(g); eb = None
            except Exception as ex: b = None; eb = ex
            tx += time.time() - t
            if a is None or b is None:
                if (a is None) != (b is None): nd += 1; print('  raise mismatch', e, repr(ea)[:80], repr(eb)[:80])
                else: err += 1
                continue
            ta = np.asarray(a['tris'], float).reshape(-1, 3, 3); tb = b['tris']
            ok = ta.shape == tb.shape and ta.tobytes() == tb.tobytes()
            ok = ok and list(a['fidx']) == b['fidx'] and [None if x is None else int(x) for x in a['frs']] == b['frs'] and list(a['unsupported']) == b['unsupported'] \
                and a['faces'] == b['faces'] and {int(k): bool(v) for k, v in a['fprio'].items()} == b['fprio']
            if not ok: nd += 1; print('  DIFFERENT node of', e, 'tris', ta.shape, tb.shape, 'fidx', list(a['fidx'])[:5], b['fidx'][:5])
    bad += nd > 0
    print('%-44s %5d nodes  reference %6.2fs rust %6.2fs  both-raise %d  %s' % (os.path.basename(f)[:44], nodes, tr, tx, err, 'IDENTICAL' if not nd else 'DIFFERENT %d' % nd), flush=True)
sys.exit(1 if bad else 0)
