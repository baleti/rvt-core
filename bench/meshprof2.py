import os, sys, time, cProfile, pstats, random
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'python')); import rvt_fast as RF
f = sys.argv[1]; N = int(sys.argv[2])
d = RF.Doc(f); cen = d._census()
ids = sorted(e for e, i in d.res.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol')); random.Random(7).shuffle(ids); ids = ids[:N]
pr = cProfile.Profile(); t = time.time(); pr.enable()
for e in ids: RF.element_mesh(d, e)
pr.disable(); print('mesh', round(time.time() - t, 1))
pr.dump_stats(sys.argv[3]); p = pstats.Stats(sys.argv[3]); p.sort_stats('tottime').print_stats(30)
