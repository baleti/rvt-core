"""cache round trip: rvt_fast.extract / Model.full from the binary cache == the same without cache (canonical digests incl. types and key order). usage: diff_cache.py FILE..."""
import os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import rvt_fast as RF
from canon import digest
bad = 0
for f in sys.argv[1:]:
    os.environ.pop('RVT_CORE_NOCACHE', None); os.environ.pop('RVT_NOCACHE', None)
    m = RF.Model(f, progress=False); m.quick(); m.full()                      # make sure the caches exist
    RF._SHARED.clear()                                                       # in-process share (extract_shared) off: test the binary cache itself
    t = time.time(); m = RF.Model(f, progress=False); m.quick(); xw, pw = m.full(); tw = time.time() - t
    os.environ['RVT_CORE_NOCACHE'] = '1'
    RF._SHARED.clear(); m = RF.Model(f, progress=False); m.quick(); xc, pc = m.full()
    os.environ.pop('RVT_CORE_NOCACHE')
    nd = 0
    for k in ('recs', 'eh'):
        if list(xw[k]) != list(xc[k]): nd += 1
        nd += sum(digest(xw[k][i]) != digest(xc[k].get(i)) for i in xw[k])
    nd += sum(digest(pw[i]) != digest(pc.get(i)) for i in pw) + (list(pw) != list(pc))
    bad += nd > 0
    print('%-44s warm full %.2fs  %s' % (os.path.basename(f)[:44], tw, 'IDENTICAL' if not nd else 'DIFFERENT %d' % nd), flush=True)
sys.exit(1 if bad else 0)
