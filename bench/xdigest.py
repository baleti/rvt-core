"""digest + time of rvt_fast.extract(want_all) with the build in RVT_FAST_DIR (default python/): compare two builds. usage: xdigest.py FILE..."""
import os, sys, time, hashlib
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, os.path.join(os.path.dirname(HERE), 'tests'))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
os.environ['RVT_CORE_NOCACHE'] = '1'
import rvt_fast as RF
from canon import digest
for f in sys.argv[1:]:
    t = time.time(); x = RF.extract(f, want_all=True); dt = time.time() - t
    h = hashlib.sha1()
    for k, r in x['recs'].items(): h.update(('%r %s' % (k, digest(r))).encode())
    for k, r in sorted(x.get('eh', {}).items()): h.update(('%r %s' % (k, digest(r))).encode())
    print('%-40s %6.1fs recs %d eh %d  %s %s' % (os.path.basename(f)[:40], dt, len(x['recs']), len(x.get('eh', {})), h.hexdigest()[:16], os.environ.get('RVT_FAST_DIR', 'released')), flush=True)
