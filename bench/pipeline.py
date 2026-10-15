#!/usr/bin/env python3
"""Pipeline timings per file, oracle vs Rust fast path (one process per measurement, nothing shared).
modes:
  oracle      pure-Python reference (RVT_NOFAST=1): extract(want_all) + params + channel-103 index + element_mesh on a sample (extrapolated to all placed elements)
  fast-new    rvt_fast.Model on a file never opened before: rvt-core cache and the file's extensible-storage schema cache removed (Formats/Latest parse cache kept:
              it is shared by every file of one Revit build)
  fast-cold   as fast-new with RVT_NOCACHE=1 (no cache of any kind)
  fast-warm   second open (binary cache present)
usage: pipeline.py MODE FILE [--sample N]   -> one JSON line"""
import os, sys, time, json, glob, hashlib
mode, f = sys.argv[1], sys.argv[2]
N = int(sys.argv[sys.argv.index('--sample') + 1]) if '--sample' in sys.argv else 300
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(HERE, '..', 'python'))
T = {}

def es_cache_file(path):
    sys.path.insert(0, os.path.expanduser('~/rvt-research'))
    import olefile
    from rvtfmt import inflate_stream
    o = olefile.OleFileIO(path); d = inflate_stream(o.openstream('Global/Latest').read())[1]
    return os.path.join(os.path.expanduser('~/.cache/rvt-schema'), 'es-' + hashlib.sha1(bytes(d)).hexdigest() + '.pkl')

if mode == 'oracle':
    os.environ['RVT_NOFAST'] = '1'; os.environ['RVT_NOCACHE'] = '1'
    sys.path.insert(0, os.path.expanduser('~/rvt-research'))
    import random
    t0 = time.time()
    import rvt_entities as E, rvt_xcache, rvt_params as P, g103_get as G, rvt_elemmesh as EM
    x = E.extract(f, want_all=True); T['extract'] = time.time() - t0
    xx = dict(x); xx.pop('sch', None); rvt_xcache.get_x = lambda p: xx
    t = time.time(); par = P.params(f); T['params'] = time.time() - t
    t = time.time(); idx = G.build_index(f); T['g103_index'] = time.time() - t
    EM.GG.cached_index = lambda p: idx
    d = EM.Doc(f); cen = {'cls': {}, 'ws': {}}
    for (eid, c), r in x['recs'].items():
        cen['cls'].setdefault(eid, c)
        if c == 'WallSweep':
            try: cen['ws'][eid] = int(E.S_(r['fields'], 'm_wallSweepInfo', 'm_wallSweepType'))
            except Exception: pass
    d._cen = cen
    ids = [e for e, i in idx.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol')]
    sm = list(ids); random.Random(1).shuffle(sm); sm = sm[:N]
    t = time.time()
    for e in sm:
        try: EM.element_mesh(d, e)
        except Exception: pass
    per = (time.time() - t) / max(1, len(sm))
    T['meshes_sample'] = len(sm); T['meshes_per_element'] = per; T['meshes_all_est'] = per * len(ids); T['n_mesh_ids'] = len(ids)
    T['total_est'] = T['extract'] + T['params'] + T['g103_index'] + T['meshes_all_est']
else:
    import rvt_fast as RF
    if mode in ('fast-new', 'fast-cold'):
        for p in glob.glob(os.path.join(RF.CACHE, RF.file_key(f) + '*')): os.remove(p)
        try: os.remove(es_cache_file(f))
        except FileNotFoundError: pass
    if mode == 'fast-cold': os.environ['RVT_NOCACHE'] = '1'
    t0 = time.time()
    m = RF.Model(f)
    q = m.quick(); T['quick_table'] = time.time() - t0
    ids = m.mesh_ids(); st = m.start_meshes(ids)          # meshes run in the background while extract + params work
    x, p = m.full(); T['full_x_params'] = time.time() - t0
    first = None; n = 0; nt = 0
    for ch in st:
        if first is None: first = time.time() - t0
        for e, r in ch: n += 1; nt += len(r['tris']) if r else 0
    T['first_meshes'] = st.t_first - t0 if st.t_first else None; T['all_meshes'] = (st.t_done or time.time()) - t0; T['n_mesh_ids'] = n; T['tris'] = nt
    m.close()
print(json.dumps({'mode': mode, 'file': os.path.basename(f), 'size_mb': round(os.path.getsize(f) / 1e6, 1), **{k: (round(v, 3) if isinstance(v, float) else v) for k, v in T.items()}}))
