"""The record between two gzip members carries the next member's length (+8) 16 bytes from its end. A flip there used to be accepted silently (400-bit-flip sweep of a 2024 .rte: 4 of 8 accepted-and-changed flips were gap bytes). Uses the local 2024 .rte, else skipped."""
import os, sys, unittest, zlib, struct
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvtreadcov as R, olefile
from rvtfmt import strip_pages, GZ
SRC = os.path.expanduser('~/rvtfix/corpus/2024/71_Construction-DefaultMetric.rte')
@unittest.skipUnless(os.path.exists(SRC), 'sample absent')
class Gap(unittest.TestCase):
    def test_gap_length_flip(self):
        raw = olefile.OleFileIO(SRC).openstream('Partitions/59').read(); data = strip_pages(raw)
        i = data.find(GZ); d = zlib.decompressobj(-15); d.decompress(memoryview(data)[i + 10:]); e = len(data) - len(d.unused_data) + 8
        j = data.find(GZ, e); self.assertEqual(j - e, 32)
        self.assertEqual(R.check_members(raw) > 3, True)
        # stored offset of byte e+16 (first page: payload = stored while below 64896)
        self.assertLess(e + 16, 64896)
        x = bytearray(raw); x[e + 16] ^= 0x04
        with self.assertRaises(ValueError): R.check_members(bytes(x))
if __name__ == '__main__': unittest.main()
