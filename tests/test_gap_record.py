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
class Gap2(unittest.TestCase):
    def _raw(self):
        raw = olefile.OleFileIO(SRC).openstream('Partitions/59').read(); data = strip_pages(raw)
        i = data.find(GZ); d = zlib.decompressobj(-15); d.decompress(memoryview(data)[i + 10:]); e = len(data) - len(d.unused_data) + 8
        return raw, e
    def test_prev_length_flip(self):
        raw, e = self._raw(); x = bytearray(raw); x[e + 2] ^= 0x08
        with self.assertRaises(ValueError): R.check_members(bytes(x))
    def test_tag_flip(self):
        raw, e = self._raw(); x = bytearray(raw); x[e] ^= 0x01
        with self.assertRaises(ValueError): R.check_members(bytes(x))
    def test_clean(self):
        raw, e = self._raw(); self.assertGreater(R.check_members(raw), 3)
class Gap3(Gap2):
    """32-byte record words: count @12 and x @20 satisfy len(next plaintext) - x == count*k (k 8/12/16/20); sweeps of 2020/2024/2026 accepted flips there"""
    def test_count_flip(self):
        raw, e = self._raw(); x = bytearray(raw); x[e + 12] ^= 0x01
        with self.assertRaises(ValueError): R.check_members(bytes(x))
    def test_x_flip(self):
        raw, e = self._raw(); x = bytearray(raw); x[e + 20] ^= 0x04
        with self.assertRaises(ValueError): R.check_members(bytes(x))
class Gap4(Gap2):
    """the 220-byte record carries the same count/x words 20/12 bytes from its end"""
    def test_220_count_and_x(self):
        raw = olefile.OleFileIO(SRC).openstream('Partitions/59').read(); data = strip_pages(raw)
        pos = pe = 0; found = None
        while True:
            i = data.find(GZ, pos)
            if i < 0: break
            d = zlib.decompressobj(-15); d.decompress(memoryview(data)[i + 10:]); e = len(data) - len(d.unused_data) + 8
            if pe and i - pe == 220 and (i - 20) // 64896 == (i - 1) // 64896: found = i; break
            pe = e; pos = e
        if found is None: self.skipTest('no 220-byte gap in the first page')
        for off in (found - 20, found - 12):
            off += off // 64896 * 353   # stripped offset -> stored offset (353-byte page trailer after each 64896-byte payload)
            x = bytearray(raw); x[off] ^= 0x04
            with self.assertRaises(ValueError): R.check_members(bytes(x))
class Gap5(Gap2):
    """last two words of every gap record are (101|102|103, 0); sweep of a 2026 .rte accepted a flip in the 101 word"""
    def test_end_word_flip(self):
        raw, e = self._raw(); x = bytearray(raw); x[e + 25] ^= 0x01     # inside the 101 word of the first 32-byte record
        with self.assertRaises(ValueError): R.check_members(bytes(x))
    def test_end_pad_flip(self):
        raw, e = self._raw(); x = bytearray(raw); x[e + 29] ^= 0x01
        with self.assertRaises(ValueError): R.check_members(bytes(x))
