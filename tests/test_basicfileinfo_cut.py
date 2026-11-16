"""BasicFileInfo cut short (directory entry intact) must be an open error in rvtreadcov (item 152: 50 and 95 percent cuts read ok)."""
import os, sys, subprocess, json, pytest
SRC = os.path.expanduser('~/c1x-work/tail98c/2023_t98c_clean.rvt')
WL = os.path.expanduser('~/rvt-research/write-lab')
pytestmark = pytest.mark.skipif(not (os.path.exists(SRC) and os.path.isdir(WL)), reason='no sample / write-lab')
TOOL = os.path.join(os.path.dirname(__file__), '..', 'tools', 'rvtreadcov.py')

def _open(path):
    out = subprocess.run([sys.executable, TOOL, path], capture_output=True, text=True).stdout.strip().splitlines()
    return json.loads(out[-1])['open']

@pytest.mark.parametrize('pct', [50, 95])
def test_cut(pct, tmp_path):
    sys.path[:0] = [WL]; import cfb
    assert _open(SRC) == 'ok'
    c = cfb.CFB(open(SRC, 'rb').read()); e = c.streams()['BasicFileInfo']; e.data = e.data[:len(e.data) * pct // 100]
    p = str(tmp_path / 'x.rvt'); cfb.write(c, p)
    assert _open(p).startswith('error:')
