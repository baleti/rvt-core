#!/usr/bin/env python3
"""profile element_mesh on a sample of placed elements (viewer-like selection); no cache written. usage: meshprof.py FILE N OUT.prof"""
import os, sys, time, cProfile, pstats, random
os.environ['RVT_NOCACHE'] = '1'; os.environ['RVT_GEOM'] = '1'
sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_entities as E, g103_get as G, rvt_elemmesh as EM
f = sys.argv[1]; N = int(sys.argv[2])
t = time.time(); x = E.extract(f, want_all=True, slim=lambda c: c == 'WallSweep'); print('census-extract', round(time.time() - t, 1))
cen = {'cls': {}, 'ws': {}}
for (eid, c), r in x['recs'].items():
    cen['cls'].setdefault(eid, c)
    if c == 'WallSweep':
        try: cen['ws'][eid] = int(E.S_(r['fields'], 'm_wallSweepInfo', 'm_wallSweepType'))
        except Exception: pass
t = time.time(); idx = G.build_index(f); print('g103', round(time.time() - t, 1))
EM.GG.cached_index = lambda p: idx
d = EM.Doc(f); d._cen = cen
ids = [e for e, i in idx.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol')]
random.seed(1); random.shuffle(ids); ids = ids[:N]
print(len(ids), 'elements')
pr = cProfile.Profile(); t = time.time(); pr.enable()
nt = 0
for e in ids:
    try: nt += len(EM.element_mesh(d, e)['tris'])
    except Exception as ex: pass
pr.disable(); print('mesh', round(time.time() - t, 1), 'tris', nt)
pr.dump_stats(sys.argv[3])
p = pstats.Stats(sys.argv[3]); p.sort_stats('cumtime').print_stats(40)
