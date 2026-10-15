"""Differential test: rvt_fast.params (Rust Src/resolve/ifc kernels) vs rvt_params.params (oracle) on the same extraction (rvt_fast.extract, itself
diff-tested), so only the parameter layer is compared. usage: diff_params.py FILE...   exit 1 on any difference"""
import os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import rvt_fast as RF, rvt_xcache, rvt_params as P
from canon import digest
bad = 0
for f in sys.argv[1:]:
    x = RF.extract(f, want_all=True); x.pop('sch', None); rvt_xcache.get_x = lambda p, x=x: x; rvt_xcache.get_x_ro = rvt_xcache.get_x
    t = time.time(); a = P.params(f); ta = time.time() - t
    t = time.time(); b = RF.params(f, x); tb = time.time() - t                       # same extraction (as Model.full): one copy in memory
    nd = sum(digest(a[k]) != digest(b.get(k)) for k in a) + len(set(b) - set(a)) + (list(a) != list(b))
    bad += nd > 0
    if os.environ.get('RVT_CORE_PARAMS_MIRROR') and RF._params_drift(): nd = nd or 'DRIFT'          # reference rvt_params.py differs from the ported one: this run used the reference loop
    print('%-48s %6d elements  oracle %5.2fs fast %5.2fs  %s' % (os.path.basename(f)[:48], len(a), ta, tb, 'IDENTICAL' if not nd else ('NOT TESTED (rvt_params.py drift: reference loop ran)' if nd == 'DRIFT' else 'DIFFERENT %d' % nd)), flush=True)
sys.exit(1 if bad else 0)
