"""g103 robustness: flip/zero/truncate bytes of a STORED partition stream (raw deflate has no checksum, so many mutants inflate to garbage that reaches the group/chain/record parsers)
and run rd.g103_load + g103_record on them in-process. Result or Python exception = fine; PanicException = PANIC. usage: fuzz_g103.py FILE [N] [SEED]  (host3-light: MemoryMax=3G)"""
import sys, os, random, time
sys.path.insert(0, os.path.expanduser('~/rvt-research')); sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.expanduser('~/src/rvt-core/python'))
import rvt_fast as R, rvt_scan as S, rvt_decode as D
f = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 20; random.seed(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
o, cls, ids = S.load(f); W = S.id_size(cls); sch = D.Schema(cls)
pns = sorted(x for x in ['/'.join(e) for e in o.listdir(streams=True)] if x.startswith('Partitions/'))
pn = pns[-1]; st0 = o.openstream(pn).read(); print('stream', pn, len(st0), flush=True)
panics = 0
for i in range(n):
    b = bytearray(st0); k = random.choice(['flip', 'zero', 'trunc', 'ff'])
    if k == 'flip':
        for _ in range(random.choice([1, 10, 200])): b[random.randrange(len(b))] ^= 1 << random.randrange(8)
    elif k == 'zero':
        a = random.randrange(len(b) - 300); b[a:a + random.choice([4, 64, 300])] = bytes(random.choice([4, 64, 300]))[:len(b[a:a + 4])] if False else bytes(len(b[a:a + 64]))
    elif k == 'ff':
        for _ in range(random.choice([5, 100])): b[random.randrange(len(b))] = 0xFF
    else: b = b[:random.randrange(len(b) // 3, len(b))]
    rd = R.make_recdecoder(sch, W, {}); t = time.time()
    try:
        ent = rd.g103_load(pn, bytes(b)); nr = 0
        for e in ent[:200]:
            try: rd.g103_record(pn, e[1], e[2]); nr += 1
            except Exception: pass
        res = 'ok %d entries %d recs' % (len(ent), nr)
    except Exception as e: res = 'exc ' + type(e).__name__
    except BaseException as e: res = 'PANIC ' + type(e).__name__ + ' ' + str(e)[:100]; panics += 1
    print(i, k, res, '%.1fs' % (time.time() - t), flush=True)
print('panics', panics)
