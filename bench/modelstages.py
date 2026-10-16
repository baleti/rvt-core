"""rvt_fast.Model stages as the viewer runs them (quick, full = extraction + params) with the stage times and the cProfile top of the full model. usage: modelstages.py FILE"""
import os, sys, time, cProfile, pstats
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
os.environ['RVT_CORE_NOCACHE'] = '1'
import rvt_fast as RF
f = sys.argv[1]
t = time.time(); m = RF.Model(f); q = m.quick(); print('quick %.1fs' % (time.time() - t), flush=True)
pr = cProfile.Profile(); pr.enable(); t = time.time(); d = m.full(); print('full %.1fs' % (time.time() - t), flush=True); pr.disable()
pstats.Stats(pr).sort_stats('cumulative').print_stats(16)
