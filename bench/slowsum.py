import sys, re, collections
n = collections.Counter(); t = collections.Counter(); tot = 0; offs = []
for ln in sys.stdin:
    m = re.match(r'rvt_core slow candidate at (\d+) eid (\d+) tags (\S+) (\S+) ([\d.]+)(ms|s|µs) accepted (\w+) (\w+)', ln)
    if not m: continue
    d = float(m.group(5)) * {'ms': 1e-3, 's': 1, 'µs': 1e-6}[m.group(6)]
    k = (m.group(3), m.group(4), m.group(7), m.group(8), int(m.group(2)) if int(m.group(2)) < 50 else 'big')
    n[k] += 1; t[k] += d; tot += d; offs.append(int(m.group(1)))
print('slow candidates', sum(n.values()), 'total cpu s %.1f' % tot, 'offsets %d..%d' % (min(offs), max(offs)) if offs else '')
for k, v in t.most_common(12): print('%7.1fs %5d  %s' % (v, n[k], k))
