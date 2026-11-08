"""Genuine Autodesk sample families (2020..2026) must read without a stream error: regression for the ElemTable duplicate-id rule misreading 2024+ RFA row layouts. Skipped if absent."""
import os, sys, glob, unittest
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvtreadcov as R
FILES = sorted(glob.glob(os.path.expanduser('~/rvt-samples/racbasicsamplefamily-202*.rfa')))
@unittest.skipUnless(FILES, 'samples absent')
class Genuine(unittest.TestCase):
    def test_no_error(self):
        for f in FILES:
            bad = [k for k, v in R.stream_inventory(f).items() if v[1].startswith('error')]
            self.assertEqual(bad, [], f)
    def test_elemtable_unknown_layout_is_not_an_error(self):
        import struct
        a = struct.pack('<HI', 0, 10) + bytes(40 * 10 + 600)   # 40-byte rows plus a long tail: layout unrecognised, must not report duplicates
        self.assertIsNone(R.elemtable_dups(a))
if __name__ == '__main__': unittest.main()
