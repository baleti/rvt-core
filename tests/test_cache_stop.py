"""Regression: a binary cache written without geometry decoding (RVT_GEOM empty) must not be served to a RVT_GEOM=1 process.
usage: python tests/test_cache_stop.py FILE.rvt"""
import os, sys, subprocess, tempfile, glob
SAMPLE = (glob.glob(os.path.expanduser('~/rvt-research/corpus/2022/30_rst_basic*.rvt')) or [None])[0]
def run(f):
    code = ("import sys,os;sys.path.insert(0,os.environ['RVT_FAST_DIR']);sys.path.insert(1,os.path.expanduser('~/rvt-research'))\n"
            "import rvt_fast as F\nx=F.extract(sys.argv[1],want_all=True)\n"
            "print(sum(1 for v in x['recs'].values() if v.get('complete') is False))")
    with tempfile.TemporaryDirectory() as cd:
        env = dict(os.environ, RVT_CORE_CACHE=cd, RVT_FAST_DIR=os.environ.get('RVT_FAST_DIR', os.path.join(os.path.dirname(__file__), '..', 'python')))
        env.pop('RVT_CORE_NOCACHE', None)
        n = [int(subprocess.check_output([sys.executable, '-c', code, f], env=dict(env, RVT_GEOM=g)).split()[-1]) for g in ('', '1', '1')]
    print('incomplete (no-geom, geom, geom cached):', n)
    assert n[0] > 0 and n[1] == 0 and n[2] == 0, 'cache served across STOP sets'
    print('OK')
    return n

def test_cache_stop():
    import pytest
    if SAMPLE is None: pytest.skip('corpus sample not present')
    run(SAMPLE)

if __name__ == '__main__':
    run(sys.argv[1])
