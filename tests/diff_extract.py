"""Differential test: rvt_fast.extract (Rust) vs rvt_entities.extract (Python oracle, pure-Python decoder RVT_NOFAST=1, RVT_WORKERS=1) on corpus files.
Each side runs in its own process and reports a canonical digest per record (types, key order, NaN-safe floats); digests are compared, first differences printed.
usage: diff_extract.py [--want-all|--default|--slim] [--oracle-workers N] FILE...    exit 1 on any difference"""
import os, sys, subprocess, json, hashlib, time
HERE = os.path.dirname(os.path.abspath(__file__))
mode = 'want_all' if '--want-all' in sys.argv else ('slim' if '--slim' in sys.argv else 'default')
ow = sys.argv[sys.argv.index('--oracle-workers') + 1] if '--oracle-workers' in sys.argv else '1'
files = [a for a in sys.argv[1:] if not a.startswith('--') and a != ow]

def canon(x, out):
    t = type(x).__name__
    if isinstance(x, dict):
        out.append('{'); [ (out.append(repr(k)), out.append(':'), canon(v, out), out.append(',')) for k, v in x.items() ]; out.append('}')
    elif isinstance(x, (list, tuple)):
        out.append('[' if isinstance(x, list) else '('); [(canon(v, out), out.append(',')) for v in x]; out.append(']' if isinstance(x, list) else ')')
    elif isinstance(x, float): out.append('f' + (x.hex() if x == x else 'nan'))
    else: out.append(t + ':' + repr(x))

def digest_side(side, f):
    sys.path.insert(0, '/home/user1/rvt-research'); os.environ['RVT_GEOM'] = '1'
    if side == 'oracle':
        os.environ['RVT_NOFAST'] = '1'; os.environ['RVT_WORKERS'] = ow
        import rvt_entities as E; fn = E.extract
    else:
        sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); import rvt_fast as RF; fn = RF.extract
    kw = {'want_all': True} if mode == 'want_all' else ({'want_all': True, 'slim': lambda c: c == 'WallSweep'} if mode == 'slim' else {})
    t = time.time(); x = fn(f, **kw); dt = time.time() - t
    rd = {}
    for k, r in x['recs'].items():
        o = []; canon(r, o); rd[repr(k)] = hashlib.sha1(''.join(o).encode()).hexdigest()[:16]
    ed = {}
    for k, v in x['eh'].items():
        o = []; canon(v, o); ed[repr(k)] = hashlib.sha1(''.join(o).encode()).hexdigest()[:16]
    json.dump({'secs': dt, 'recs': rd, 'eh': ed, 'rorder': list(rd), 'eorder': list(ed), 'W': x['W']}, sys.stdout)

if len(sys.argv) > 2 and sys.argv[1] == '--side':
    digest_side(sys.argv[2], sys.argv[3]); sys.exit(0)

bad = 0
for f in files:
    res = {}
    for side in ('oracle', 'fast'):
        p = subprocess.run([sys.executable, __file__, '--side', side, f, '--' + mode.replace('_', '-'), '--oracle-workers', ow], capture_output=True, text=True)
        if p.returncode: print('FAIL', side, os.path.basename(f), p.stderr[-800:]); res = None; break
        res[side] = json.loads(p.stdout)
    if res is None: bad += 1; continue
    a, b = res['oracle'], res['fast']; nd = 0
    for kind in ('recs', 'eh'):
        ka, kb = set(a[kind]), set(b[kind])
        miss = ka - kb; extra = kb - ka; diff = [k for k in ka & kb if a[kind][k] != b[kind][k]]
        if miss or extra or diff: nd += len(miss) + len(extra) + len(diff); print('  %s %s: missing %d extra %d differ %d  e.g. %s' % (os.path.basename(f), kind, len(miss), len(extra), len(diff), (sorted(miss)[:2], sorted(extra)[:2], diff[:3])))
    if a['rorder'] != b['rorder'] or a['eorder'] != b['eorder']: print('  %s: insertion order differs' % os.path.basename(f)); nd += 1
    bad += nd > 0
    print('%-50s %s recs %6d eh %6d  oracle %6.1fs fast %5.1fs  %s' % (os.path.basename(f)[:50], mode, len(a['recs']), len(a['eh']), a['secs'], b['secs'], 'IDENTICAL' if nd == 0 else 'DIFFERENT'), flush=True)
sys.exit(1 if bad else 0)
