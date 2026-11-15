"""rvt_scan.load on magic + 504 zero bytes (header only, FAT sector missing) must say 'corrupt CFB', not an array/struct internal error."""
import os, sys, tempfile
sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import pytest
rvt_scan = pytest.importorskip('rvt_scan')
def test_short_header():
    p = os.path.join(tempfile.mkdtemp(), 'c.rvt'); open(p, 'wb').write(bytes.fromhex('D0CF11E0A1B11AE1') + bytes(504))
    with pytest.raises(ValueError, match='corrupt CFB'): rvt_scan.load(p)
