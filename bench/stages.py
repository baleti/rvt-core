#!/usr/bin/env python3
"""Cold pipeline timing per stage, as the viewer runs it (reader.load -> extract + params, g103 index, census, element_mesh).
No cache is read or written (RVT_NOCACHE=1, xcache bypassed).  usage: stages.py FILE [--prof OUT] [--mesh N]"""
import os, sys, time, cProfile, pstats
os.environ['RVT_NOCACHE'] = '1'; os.environ['RVT_GEOM'] = '1'
sys.path.insert(0, os.path.expanduser('~/rvt-research'))
f = sys.argv[1]; prof = sys.argv[sys.argv.index('--prof') + 1] if '--prof' in sys.argv else None
NM = int(sys.argv[sys.argv.index('--mesh') + 1]) if '--mesh' in sys.argv else 300
T = {}
def st(name, fn, *a, **k):
    t = time.time(); r = fn(*a, **k); T[name] = round(time.time() - t, 2); print('%-12s %7.2fs' % (name, T[name]), flush=True); return r
pr = cProfile.Profile() if prof else None
if pr: pr.enable()
import rvt_entities as E, rvt_xcache, rvt_params as P, g103_get as G, rvt_scan as S, rvt_es as ES
x = st('extract', E.extract, f, want_all=True); x.pop('sch', None)
rvt_xcache.get_x = lambda p: x
par = st('params', P.params, f)
idx = st('g103_index', G.build_index, f)
import rvt_elemmesh as EM
EM.GG.cached_index = lambda p: idx
d = EM.Doc(f)
cen = st('census', d._census) if '--census' in sys.argv else None
ids = sorted(e for e, i in idx.items() if i.get('bbox'))[:NM]
def meshes():
    n = 0
    for e in ids:
        try: m = EM.element_mesh(d, e); n += 1
        except Exception: pass
    return n
st('mesh_%d' % len(ids), meshes)
if pr:
    pr.disable(); pr.dump_stats(prof)
print('records', len(x['recs']), 'headers', len(x['eh']), 'params', len(par), 'g103', len(idx), 'with bbox', sum(1 for i in idx.values() if i.get('bbox')))
print(T)
