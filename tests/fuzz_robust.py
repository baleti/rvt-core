"""robustness: flip/truncate bytes of a small file and run the fast path (extract, params-free) in a subprocess; it must exit 0 or raise a Python exception, never crash (signal), hang or exceed memory.
usage: fuzz_robust.py FILE OUTDIR [N] [SEED]   (host3-light: each case < 2 s, MemoryMax 2G)"""
import sys, os, random, subprocess
f, out = sys.argv[1], sys.argv[2]; n = int(sys.argv[3]) if len(sys.argv) > 3 else 20; random.seed(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
os.makedirs(out, exist_ok=True); data = open(f, 'rb').read(); bad = 0
code = "import sys,os;sys.path.insert(0,os.path.expanduser('~/rvt-research'));sys.path.insert(0,os.path.expanduser('~/src/rvt-core/python'));import rvt_fast as R\ntry:\n  x=R.extract(sys.argv[1],want_all=True);print('ok',len(x['recs']))\nexcept Exception as e: print('exc',type(e).__name__)"
for i in range(n):
    b = bytearray(data); kind = random.choice(['flip', 'trunc', 'zero'])
    if kind == 'flip':
        for _ in range(random.choice([1, 8, 64])): b[random.randrange(len(b))] ^= 1 << random.randrange(8)
    elif kind == 'trunc': b = b[:random.randrange(len(b) // 4, len(b))]
    else:
        a = random.randrange(len(b) - 5000); b[a:a + random.choice([16, 512, 4096])] = bytes(random.choice([16, 512, 4096]) and 0 for _ in range(0)) or b[a:a + 16].replace(b[a:a + 16], bytes(16))
    p = os.path.join(out, 'c%03d.rvt' % i); open(p, 'wb').write(b)
    env = dict(os.environ, RVT_CORE_NOCACHE='1', RVT_THREADS='4')
    try:
        r = subprocess.run(['systemd-run', '--user', '--scope', '-q', '-p', 'MemoryMax=2G', '-p', 'MemorySwapMax=0', 'nice', '-n', '19', sys.executable, '-c', code, p], capture_output=True, text=True, timeout=120, env=env)
        res = r.stdout.strip() or ('rc%d %s' % (r.returncode, r.stderr.strip()[-120:]))
    except subprocess.TimeoutExpired: res = 'TIMEOUT'
    flag = '' if res.startswith(('ok', 'exc')) else '  <-- BAD'; bad += bool(flag)
    print(i, kind, res, flag, flush=True); os.remove(p)
print('bad', bad); sys.exit(1 if bad else 0)
