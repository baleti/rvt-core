"""An emptied core stream (directory entry intact, 0 bytes) must be an open error in rvtreadcov, not 'ok'."""
import os, sys, subprocess, json, tempfile, pytest
SRC = os.path.expanduser('~/c1x-work/tail98c/2023_t98c_clean.rvt')
WL = os.path.expanduser('~/rvt-research/write-lab')
pytestmark = pytest.mark.skipif(not (os.path.exists(SRC) and os.path.isdir(WL)), reason='no sample / write-lab')
TOOL = os.path.join(os.path.dirname(__file__), '..', 'tools', 'rvtreadcov.py')

def _open(path):
    out = subprocess.run([sys.executable, TOOL, path], capture_output=True, text=True).stdout.strip().splitlines()
    return json.loads(out[-1])['open']

@pytest.mark.parametrize('name', ['Global/History', 'Formats/Latest', 'Global/ElemTable'])
def test_emptied(name, tmp_path):
    sys.path[:0] = [WL]; import cfb
    assert _open(SRC) == 'ok'
    c = cfb.CFB(open(SRC, 'rb').read()); c.streams()[name].data = b''
    p = str(tmp_path / 'e.rvt'); cfb.write(c, p)
    assert _open(p).startswith('error:ValueError core stream emptied')
