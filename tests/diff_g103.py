"""Differential test: rvt_fast.build_index / get_record (Rust) vs g103_get.build_index / get_record (Python oracle, pure-Python decoder) on corpus files.
Index entries compared with == (+ key order); get_record compared on a sample of entries. usage: diff_g103.py [--n N] FILE...   exit 1 on any difference"""
import os, sys, subprocess, json, hashlib, time, random
HERE = os.path.dirname(os.path.abspath(__file__))
N = int(sys.argv[sys.argv.index('--n') + 1]) if '--n' in sys.argv else 200
sys.path.insert(0, HERE); from canon import canon

def side(which, f):
    sys.path.insert(0, '/home/user1/rvt-research'); os.environ['RVT_GEOM'] = '1'
    if which == 'oracle':
        os.environ['RVT_NOFAST'] = '1'; import g103_get as G; t = time.time(); idx = G.build_index(f); dt = time.time() - t; ole, dec = G.load_dec(f); get = lambda i: G.get_record(f, i, ole, dec)
    else:
        sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python')); import rvt_fast as RF; t = time.time(); idx = RF.build_index(f); dt = time.time() - t; get = lambda i: RF.get_record(f, i)
    dig = {}
    for k, v in idx.items():
        o = []; canon(v, o); dig[repr(k)] = hashlib.sha1(''.join(o).encode()).hexdigest()[:16]
    ks = sorted(idx); random.Random(3).shuffle(ks); rr = {}
    for k in ks[:N]:
        try: r = get(idx[k]); o = []; canon(r, o); rr[repr(k)] = hashlib.sha1(''.join(o).encode()).hexdigest()[:16]
        except Exception as e: rr[repr(k)] = 'EXC ' + type(e).__name__
    json.dump({'secs': dt, 'idx': dig, 'order': list(dig), 'rec': rr}, sys.stdout)

if __name__ == '__main__':
    if sys.argv[1] == '--side': side(sys.argv[2], sys.argv[3]); sys.exit(0)
    files = [a for a in sys.argv[1:] if not a.startswith('--') and a != str(N)]
    bad = 0
    for f in files:
        res = {}
        for w in ('oracle', 'fast'):
            p = subprocess.run([sys.executable, __file__, '--side', w, f, '--n', str(N)], capture_output=True, text=True)
            if p.returncode: print('FAIL', w, os.path.basename(f), p.stderr[-600:]); res = None; break
            res[w] = json.loads(p.stdout)
        if res is None: bad += 1; continue
        a, b = res['oracle'], res['fast']
        di = [k for k in set(a['idx']) | set(b['idx']) if a['idx'].get(k) != b['idx'].get(k)]
        dr = [k for k in a['rec'] if a['rec'][k] != b['rec'].get(k)]
        ok = not di and not dr and a['order'] == b['order']
        bad += not ok
        print('%-48s index %6d (%d differ) records %d (%d differ) order %s  oracle %5.1fs fast %5.2fs  %s' % (os.path.basename(f)[:48], len(a['idx']), len(di), len(a['rec']), len(dr), a['order'] == b['order'], a['secs'], b['secs'], 'IDENTICAL' if ok else 'DIFFERENT'), flush=True)
    sys.exit(1 if bad else 0)
