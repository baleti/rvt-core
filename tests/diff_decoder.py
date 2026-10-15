"""Differential test of the Rust record decoder against rvt_decode.Dec (Python oracle): same dict (==), same end offset, same exception behaviour.
usage: diff_decoder.py FILE... [--n N]"""
import time, sys, os, random, struct, glob, collections
sys.path.insert(0, '/home/user1/rvt-research'); sys.path.insert(0, '/home/user1/src/rvt-core/python'); os.environ['RVT_GEOM'] = '1'
import numpy as np, rvt_scan as S, rvt_decode as D, rvt2text as T, rvt_inventory as I
import rvt_core as RC
args = [a for a in sys.argv[1:] if not a.startswith('--')]
N = int(sys.argv[sys.argv.index('--n') + 1]) if '--n' in sys.argv else 1500
if '--n' in sys.argv: args = [a for a in args if a != sys.argv[sys.argv.index('--n') + 1]]
def deep(a, b, path):
    if type(a) != type(b): return '%s: type %s vs %s (%r vs %r)' % (path, type(a).__name__, type(b).__name__, str(a)[:40], str(b)[:40])
    if isinstance(a, dict):
        for k in a:
            if k not in b: return '%s.%s missing in rust' % (path, k)
            r = deep(a[k], b[k], path + '.' + k)
            if r: return r
        for k in b:
            if k not in a: return '%s.%s extra in rust' % (path, k)
        if list(a) != list(b): return '%s key order' % path
        return None
    if isinstance(a, (list, tuple)):
        if len(a) != len(b): return '%s: len %d vs %d' % (path, len(a), len(b))
        for i, (x, y) in enumerate(zip(a, b)):
            r = deep(x, y, '%s[%d]' % (path, i))
            if r: return r
        return None
    if isinstance(a, float) and a != a and b != b: return None
    return None if a == b else '%s: %r vs %r' % (path, a, b)
tot = collections.Counter()
for f in args:
    o, cls, ids = S.load(f); W = S.id_size(cls); sch = D.Schema(cls)
    pyd = D.Dec(sch, idsize=W)
    import rvt_es as ES
    try: pyd.es_schemas = ES.load(f)[0]
    except Exception: pyd.es_schemas = {}
    rd = RC.RecDecoder(cls, W, list(pyd.STOP), D.STRCAP, D.EID, D.Desync); rd.set_es(pyd.es_schemas)
    et = I.element_tags(cls); ida = np.array(sorted(ids), dtype=np.uint32 if W == 4 else np.uint64); F = '<I' if W == 4 else '<Q'
    rnd = random.Random(5); samples = []
    for pn in sorted('/'.join(e) for e in o.listdir(streams=True) if e[0] == 'Partitions'):
        ch = T.joined(o.openstream(pn).read()); cand = S.header_candidates(ch, ida, W)
        for i in rnd.sample(cand, min(len(cand), max(1, N // 4))): samples.append((ch, i))
    rnd.shuffle(samples)
    for ch, i in samples[:N]:
        for short in (False, True):
            t0 = time.time()
            try: a = pyd.record(ch, i, short); ea = None
            except Exception as e: a = None; ea = type(e).__name__ + ':' + str(e)[:80]
            tot['t_py'] += time.time() - t0; t0 = time.time()
            try: b = rd.record(ch, i, short); eb = None
            except Exception as e: b = None; eb = type(e).__name__ + ':' + str(e)[:80]
            tot['t_rs'] += time.time() - t0; tot['cases'] += 1
            if ea is not None or eb is not None:
                if (ea is None) != (eb is None): tot['exception mismatch'] += 1; print('EXC MISMATCH', os.path.basename(f), i, short, ea, eb)
                else: tot['both raise'] += 1
                continue
            if a[1] == b[1] and deep(a[0], b[0], 'rec') is None: tot['identical'] += 1
            else:
                tot['DIFF'] += 1
                ra, rb = a[0], b[0]
                keys = [k for k in set(ra) | set(rb) if ra.get(k) != rb.get(k)]
                print('DIFF', os.path.basename(f), i, short, 'end', a[1], b[1], 'keys', keys, 'err', ra.get('error'), '|', rb.get('error'), '|', deep(ra.get('fields'), rb.get('fields'), 'fields'))
    print(os.path.basename(f), {k: (round(v, 2) if isinstance(v, float) else v) for k, v in tot.items()}, flush=True)
sys.exit(1 if tot['DIFF'] or tot['exception mismatch'] else 0)
