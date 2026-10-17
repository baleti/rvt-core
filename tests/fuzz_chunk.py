"""decoder robustness: mutate the INFLATED partition bytes (random byte flips / zeroed runs / truncation) and run rd.extract_chunk + candidates on them in-process.
Outcomes: result or Python exception = fine; PanicException (pyo3 BaseException) = reported as PANIC (a Rust panic the adapters must not let through); a crash/hang kills the run.
usage: fuzz_chunk.py FILE [N] [SEED]   (host3-light: run under systemd-run MemoryMax=3G)"""
import sys, os, random, time
sys.path.insert(0, os.path.expanduser('~/rvt-research')); sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python'))
import rvt_fast as R, rvt_scan as S, rvt_decode as D, rvt2text as T, rvt_inventory as I
f = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 20; random.seed(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
o, cls, ids = S.load(f); W = S.id_size(cls); sch = D.Schema(cls)
EH = sch.by['ElementHeader'] + 12; names = {i + 12: c['name'] for i, c in enumerate(cls)}; et = I.element_tags(cls)
wl = sorted(t for t in names if t in et and names[t] != 'DBView')
pns = sorted(x for x in ['/'.join(e) for e in o.listdir(streams=True)] if x.startswith('Partitions/'))
ch0 = bytes(T.joined(o.openstream(pns[-1]).read())); print('chunk', len(ch0), flush=True)
panics = 0
for i in range(n):
    b = bytearray(ch0); k = random.choice(['flip', 'zero', 'trunc', 'ff'])
    if k == 'flip':
        for _ in range(random.choice([10, 200, 5000])): b[random.randrange(len(b))] ^= 1 << random.randrange(8)
    elif k == 'zero':
        for _ in range(random.choice([1, 20])):
            a = random.randrange(len(b) - 300); b[a:a + random.choice([4, 64, 300])] = bytes(len(b[a:a + random.choice([4, 64, 300])]) and 4)
    elif k == 'ff':
        for _ in range(random.choice([10, 500])): b[random.randrange(len(b))] = 0xFF
    else: b = b[:random.randrange(len(b) // 3, len(b))]
    rd = R.make_recdecoder(sch, W, None); rd.set_ids(sorted(ids)); t = time.time()
    try: r = rd.extract_chunk(bytes(b), wl, EH, None, None); res = 'ok %d recs' % len(r[0])
    except Exception as e: res = 'exc ' + type(e).__name__
    except BaseException as e: res = 'PANIC ' + type(e).__name__ + ' ' + str(e)[:100]; panics += 1
    print(i, k, res, '%.1fs' % (time.time() - t), flush=True)
print('panics', panics)
