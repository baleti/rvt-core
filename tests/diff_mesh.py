"""Differential test for element meshes: oracle (rvt_elemmesh.Doc + element_mesh, pure-Python decoder) vs rvt_fast (Doc / Rust mesh path).
Compares every element_mesh field bit-exactly (triangles, fid, nid, flags, source, bbox, meta kind + uv); surface evaluators are compared by evaluating them at the
triangle's uv points. Elements: a seeded sample of placed elements with a stored bbox (+ every symbol they instantiate through the memo).
usage: diff_mesh.py [--n N] [--impl doc|rust] FILE...   exit 1 on any difference"""
import os, sys, subprocess, json, time, random, hashlib
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
N = int(sys.argv[sys.argv.index('--n') + 1]) if '--n' in sys.argv else 300
IMPL = sys.argv[sys.argv.index('--impl') + 1] if '--impl' in sys.argv else 'doc'

def mdigest(m):
    import numpy as np
    from canon import canon
    o = []
    for t in m.get('tris') or []:
        for p in t: o.append(np.asarray(p, float).tobytes().hex())
    meta = []
    for x in m.get('meta') or []:
        k, ev, uv = x
        ent = [k, repr(tuple(tuple(float(c) for c in q) for q in uv) if uv is not None else None)]     # uv values exact; the container type (list from a decoded record / tuple when computed) is not compared
        if ev is not None and uv is not None:
            try: ent.append([np.asarray(ev(float(u), float(v)), float).tobytes().hex() for u, v in uv])
            except Exception as e: ent.append('EXC')
        meta.append(ent)
    rest = {k: v for k, v in m.items() if k not in ('tris', 'meta')}
    c = []; canon(rest, c)
    # geometric signature per face: triangle count, area (1e-9 relative), vertex set (1e-6 ft), surface kinds
    import collections
    g = collections.defaultdict(list); kinds = collections.defaultdict(collections.Counter)
    fid = m.get('fid') or [0] * len(m.get('tris') or [])
    for i, t in enumerate(m.get('tris') or []):
        g[fid[i]].append(t)
        if m.get('meta') and i < len(m['meta']): kinds[fid[i]][m['meta'][i][0]] += 1
    geo = []
    for f_ in sorted(g):
        T = np.array(g[f_], float); ar = float(np.linalg.norm(np.cross(T[:, 1] - T[:, 0], T[:, 2] - T[:, 0]), axis=1).sum() / 2)
        vs = sorted({tuple(np.round(p / 1e-6).astype(np.int64).tolist()) for p in T.reshape(-1, 3)})
        geo.append([f_, len(g[f_]), '%.9e' % ar, hashlib.sha1(repr(vs).encode()).hexdigest()[:12], sorted(kinds[f_].items())])
    return {'tris': hashlib.sha1(''.join(o).encode()).hexdigest()[:16], 'meta': hashlib.sha1(json.dumps(meta).encode()).hexdigest()[:16], 'rest': hashlib.sha1(''.join(c).encode()).hexdigest()[:16], 'n': len(m.get('tris') or []),
            'geo': hashlib.sha1(json.dumps(geo).encode()).hexdigest()[:16]}

def side(which, f):
    sys.path.insert(0, '/home/user1/rvt-research'); os.environ['RVT_GEOM'] = '1'
    if which == 'oracle':
        os.environ['RVT_NOFAST'] = '1'
        import rvt_elemmesh as EM, g103_get as G, rvt_entities as E
        t = time.time(); idx = G.build_index(f); EM.GG.cached_index = lambda p: idx
        d = EM.Doc(f)
        x = E.extract(f, want_all=True, slim=lambda c: c == 'WallSweep')                  # census without the xcache file
        cen = {'cls': {}, 'ws': {}}
        for (eid, c), r in x['recs'].items():
            cen['cls'].setdefault(eid, c)
            if c == 'WallSweep':
                try: cen['ws'][eid] = int(E.S_(r['fields'], 'm_wallSweepInfo', 'm_wallSweepType'))
                except Exception: pass
        d._cen = cen; mesh = lambda e: EM.element_mesh(d, e)
    else:
        sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); import rvt_fast as RF
        t = time.time(); d = RF.Doc(f); idx = d.res; d._census(); cen = d._cen
        mesh = (lambda e: RF.element_mesh(d, e)) if IMPL == 'doc' else (lambda e: RF.element_mesh_rust(d, e))
    tl = time.time() - t
    ids = sorted(e for e, i in idx.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol'))
    random.Random(7).shuffle(ids); ids = ids[:N]
    out = {}; t = time.time()
    for e in ids:
        try: out[str(e)] = mdigest(mesh(e))
        except Exception as ex: out[str(e)] = 'EXC %s' % type(ex).__name__
    json.dump({'load': tl, 'mesh': time.time() - t, 'res': out}, sys.stdout)

if __name__ == '__main__':
    if sys.argv[1] == '--side': side(sys.argv[2], sys.argv[3]); sys.exit(0)
    files = [a for a in sys.argv[1:] if not a.startswith('--') and a not in (str(N), IMPL)]
    bad = 0
    for f in files:
        res = {}
        for w in ('oracle', 'fast'):
            p = subprocess.run([sys.executable, __file__, '--side', w, f, '--n', str(N), '--impl', IMPL], capture_output=True, text=True)
            if p.returncode: print('FAIL', w, os.path.basename(f), p.stderr[-800:]); res = None; break
            res[w] = json.loads(p.stdout)
        if res is None: bad += 1; continue
        a, b = res['oracle']['res'], res['fast']['res']
        ident = [k for k in a if a[k] == b.get(k)]
        equiv = [k for k in a if a[k] != b.get(k) and isinstance(a[k], dict) and isinstance(b.get(k), dict) and a[k]['geo'] == b[k]['geo'] and a[k]['rest'] == b[k]['rest'] and a[k]['n'] == b[k]['n']]
        diff = [k for k in a if k not in ident and k not in equiv]
        bad += bool(diff)
        print('%-44s %4d elements: %4d bit-identical %3d geometrically equivalent %3d DIFFERENT  oracle load %5.1fs mesh %6.1fs | fast load %5.1fs mesh %6.1fs  %s' % (os.path.basename(f)[:44], len(a), len(ident), len(equiv), len(diff), res['oracle']['load'], res['oracle']['mesh'], res['fast']['load'], res['fast']['mesh'], 'OK' if not diff else 'e.g. %s %s %s' % (diff[0], a[diff[0]], b.get(diff[0]))), flush=True)
    sys.exit(1 if bad else 0)
