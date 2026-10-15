"""Differential test: rvt_core (Rust) vs the Python reference in ~/rvt-research on every corpus/sample file.
Compares (1) strip_pages, (2) joined() inflate output bytes, (3) header_candidates for each partition. Exit 1 on any difference."""
import sys, os, glob, hashlib, time, struct
sys.path.insert(0, '/home/user1/rvt-research')
import olefile, numpy as np
import rvt2text as T, rvt_scan as S, rvtfmt
import rvt_core as RC
files = sorted(glob.glob('/home/user1/rvt-research/corpus/*/*.rv*') + glob.glob('/home/user1/rvt-research/corpus/*/*.rfa') + glob.glob('/home/user1/rvt-research/samples/*.rvt') + glob.glob('/home/user1/rvt-research/samples-rfa/*.rfa'))
if len(sys.argv) > 1: files = files[::int(sys.argv[1])]
bad = 0; n = 0; fb = 0; tpy = trs = 0.0
for f in files:
    try: o = olefile.OleFileIO(f)
    except Exception: continue
    streams = ['/'.join(e) for e in o.listdir(streams=True)]
    try: o2, cls, ids = S.load(f); W = S.id_size(cls); ida = np.array(sorted(ids), dtype=np.uint32 if W == 4 else np.uint64)
    except Exception: continue
    for pn in streams:
        if not pn.startswith('Partitions/'): continue
        raw = o.openstream(pn).read()
        a = rvtfmt.strip_pages(raw); b = RC.strip_pages(raw)
        t = time.time(); j = T.joined(raw); tpy += time.time() - t
        t = time.time(); jr = RC.joined(raw); trs += time.time() - t
        ok = (a == b) and (jr is None or j == jr)
        if jr is None: fb += 1
        if ok:
            ca = S.header_candidates(j, ida, W); cb = RC.header_candidates(j, [int(x) for x in sorted(ids)], W)
            ok = ca == cb
        n += 1
        if not ok:
            bad += 1; print('DIFF', os.path.basename(f)[:30], pn, 'strip' if a != b else '', 'joined' if (jr is not None and j != jr) else '', 'cands' if (a == b and (jr is None or j == jr)) else '', 'len', len(j), len(jr) if jr is not None else None)
print('partitions compared', n, 'different', bad, 'rust fell back to python', fb, 'python joined %.1fs rust joined %.1fs' % (tpy, trs))
sys.exit(1 if bad else 0)
