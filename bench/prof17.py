import os, sys, time, cProfile, pstats
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python')); os.environ['RVT_CORE_NOCACHE'] = '1'
import rvt_fast as RF
f = sys.argv[1]
pr = cProfile.Profile(); pr.enable(); x = RF.extract(f, want_all=True); pr.disable()
pstats.Stats(pr).sort_stats('tottime').print_stats(14)
