"""Valid CFB magic + zeroed header must give a clear 'corrupt CFB' ValueError, not an olefile/array internal error."""
import os, sys, tempfile
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'pysrc'))
import rvt_fast as R
def test_bad_header():
    p = os.path.join(tempfile.mkdtemp(), 'c.rvt'); open(p, 'wb').write(bytes.fromhex('D0CF11E0A1B11AE1') + bytes(4000))
    try: R.extract(p)
    except ValueError as e: assert 'corrupt CFB' in str(e); return
    raise AssertionError('no error')
if __name__ == '__main__': test_bad_header(); print('ok')
