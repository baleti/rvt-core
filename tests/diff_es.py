"""Differential test: rvt_fast.find_schemas (Rust) vs rvt_es.find_schemas (Python oracle, pure-Python decoder) on Global/Latest of every file. exit 1 on any difference"""
import os, sys, time, glob
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import rvt_fast as RF
sys.path.insert(0, '/home/user1/rvt-research')
import olefile, fmt_parser as fp, rvt_decode as D, rvt_scan as S, rvt_es as ES
from rvtfmt import inflate_stream
from canon import digest
files = [a for a in sys.argv[1:]] or sorted(glob.glob('/home/user1/rvt-research/corpus/*/*'))
bad = 0
for f in files:
    o = olefile.OleFileIO(f); cls, _ = fp.parse(inflate_stream(o.openstream('Formats/Latest').read())[1]); sch = D.Schema(cls); W = S.id_size(cls)
    d = inflate_stream(o.openstream('Global/Latest').read())[1]
    os.environ['RVT_NOFAST'] = '1'; t = time.time(); a = ES.find_schemas(d, sch, W); ta = time.time() - t; del os.environ['RVT_NOFAST']
    t = time.time(); b = RF.find_schemas(d, sch, W); tb = time.time() - t
    ok = list(a) == list(b) and all(digest(a[k]) == digest(b[k]) for k in a)
    bad += not ok
    print('%-48s %4d schemas  oracle %5.2fs fast %5.3fs  %s' % (os.path.basename(f)[:48], len(a), ta, tb, 'IDENTICAL' if ok else 'DIFFERENT %d vs %d' % (len(a), len(b))), flush=True)
sys.exit(1 if bad else 0)
