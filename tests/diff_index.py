"""rvt_core.index_handles vs the reference walk (rvtsheets.gfx.Collector._index) on the saved-graphics records of a seeded sample of elements: same keys in the same order,
same node objects (identity). usage: diff_index.py [--n N] FILE..."""
import os, sys, random, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvt_fast as RF
n = 300
if '--n' in sys.argv: i = sys.argv.index('--n'); n = int(sys.argv[i + 1]); del sys.argv[i:i + 2]
RC = RF._rc(); bad = 0
def ref(x, out):
    if isinstance(x, dict):
        if 'handle' in x and 'obj' in x and isinstance(x.get('handle'), int) and x['handle'] != 0xFFFFFFFF: out[x['handle']] = x
        for v in x.values(): ref(v, out)
    elif isinstance(x, list):
        for v in x: ref(v, out)
for f in sys.argv[1:]:
    d = RF.Doc(f); ids = [e for e, i in d.res.items()]; random.Random(2).shuffle(ids); nd = k = 0; tr = tx = 0.0; nh = 0
    for e in ids[:n]:
        rec = d.rec(e)
        if rec is None: continue
        fields = rec['fields']; k += 1
        t = time.time(); a = {}; ref(fields, a); tr += time.time() - t
        t = time.time(); b = RC.index_handles(fields); tx += time.time() - t
        nh += len(a)
        if list(a) != list(b) or any(a[h] is not b[h] for h in a): nd += 1
    bad += nd > 0
    print('%-44s %5d records %7d handles  python %6.2fs rust %6.2fs  %s' % (os.path.basename(f)[:44], k, nh, tr, tx, 'IDENTICAL' if not nd else 'DIFFERENT %d' % nd), flush=True)
sys.exit(1 if bad else 0)
