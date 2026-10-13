"""small sampling profiler (all threads): s = Sampler(0.005).start(); ...; s.report(12, threads={'MainThread'}). Native calls holding the GIL are invisible."""
import sys, threading, traceback, collections
class Sampler:
    def __init__(s, dt=0.005): s.dt = dt; s.c = collections.defaultdict(collections.Counter); s.cum = collections.defaultdict(collections.Counter); s.stop = threading.Event()
    def _run(s):
        me = threading.get_ident()
        while not s.stop.wait(s.dt):
            names = {t.ident: t.name for t in threading.enumerate()}
            for tid, fr in sys._current_frames().items():
                if tid == me: continue
                st = traceback.extract_stack(fr); nm = names.get(tid, str(tid)); top = st[-1]
                s.c[nm]['%s:%d %s' % (top.filename.split('/')[-1], top.lineno, top.name)] += 1
                seen = set()
                for x in st:
                    k = '%s %s' % (x.filename.split('/')[-1], x.name)
                    if k not in seen: seen.add(k); s.cum[nm][k] += 1
    def start(s): threading.Thread(target=s._run, daemon=True, name='sampler').start(); return s
    def report(s, n=12, threads=None):
        s.stop.set()
        for nm in s.c:
            if threads and nm not in threads: continue
            tot = sum(s.c[nm].values()); print('== thread', nm, 'samples', tot)
            print('  self:', s.c[nm].most_common(n)); print('  cum :', s.cum[nm].most_common(n + 10))
