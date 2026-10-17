"""how often the reference's _col_grid / _subdivide_grid fire while diff_tess.py runs on FILE...: a differential run that never triggers them proves nothing. usage: count_colgrid.py FILE..."""
import sys, os, runpy
sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_mesh as RM
c = {'col': 0, 'sub': 0}; oc, os_ = RM._col_grid, RM._subdivide_grid
def f(*a, **k):
    r = oc(*a, **k); c['col'] += r is not None; return r
def g(*a, **k):
    c['sub'] += 1; return os_(*a, **k)
RM._col_grid = f; RM._subdivide_grid = g
sys.argv = ['diff_tess.py', '--n', '400'] + sys.argv[1:]
try: runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'diff_tess.py'), run_name='__main__')
except SystemExit: pass
print(c)
