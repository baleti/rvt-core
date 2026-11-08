"""TransmissionData = u32 UTF-16 char count + XML. A wrong count (or non-XML payload) must be an error; the untouched file must not. Needs the 2024 Construction .rte (local corpus), else skipped."""
import os, sys, shutil, tempfile, unittest
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvtreadcov as R, olefile
SRC = os.path.expanduser('~/rvtfix/corpus/2024/71_Construction-DefaultMetric.rte')
@unittest.skipUnless(os.path.exists(SRC), 'sample absent')
class TD(unittest.TestCase):
    def _flip(self, f):
        d = tempfile.mkdtemp(); p = os.path.join(d, 't.rte'); shutil.copy(SRC, p)
        o = olefile.OleFileIO(p, write_mode=True); raw = bytearray(o.openstream('TransmissionData').read()); f(raw)
        o.write_stream('TransmissionData', bytes(raw)); o.close(); return p
    def test_clean(self):
        p = self._flip(lambda r: None); self.assertEqual(R.stream_inventory(p)['TransmissionData'][1], 'opaque')
    def test_count_flip(self):
        p = self._flip(lambda r: r.__setitem__(0, r[0] ^ 0x10))
        self.assertTrue(R.stream_inventory(p)['TransmissionData'][1].startswith('error'))
    def test_not_xml(self):
        p = self._flip(lambda r: r.__setitem__(4, 0x41))
        self.assertTrue(R.stream_inventory(p)['TransmissionData'][1].startswith('error'))
if __name__ == '__main__': unittest.main()
class Contents(TD):
    def _flipc(self, f):
        d = tempfile.mkdtemp(); p = os.path.join(d, 't.rte'); shutil.copy(SRC, p)
        o = olefile.OleFileIO(p, write_mode=True); raw = bytearray(o.openstream('Contents').read()); f(raw)
        o.write_stream('Contents', bytes(raw)); o.close(); return p
    def test_contents_clean(self):
        self.assertEqual(R.stream_inventory(self._flipc(lambda r: None))['Contents'][1], 'opaque')
    def test_contents_magic_flip(self):
        self.assertTrue(R.stream_inventory(self._flipc(lambda r: r.__setitem__(1, r[1] ^ 1)))['Contents'][1].startswith('error'))
class Preview(unittest.TestCase):
    def _mod(self, f):
        d = tempfile.mkdtemp(); p = os.path.join(d, 't.rte'); shutil.copy(SRC, p)
        o = olefile.OleFileIO(p, write_mode=True); raw = bytearray(o.openstream('RevitPreview4.0').read()); f(raw)
        o.write_stream('RevitPreview4.0', bytes(raw)); o.close(); return p
    def test_clean(self):
        self.assertEqual(R.stream_inventory(self._mod(lambda r: None))['RevitPreview4.0'][1], 'opaque')
    def test_png_body_flip(self):
        def f(r): i = r.find(b'IDAT') + 8; r[i] ^= 0x55
        self.assertTrue(R.stream_inventory(self._mod(f))['RevitPreview4.0'][1].startswith('error'))
class History(unittest.TestCase):
    def _a(self, ids, count=None):
        import struct
        a = bytearray(106 + 4 * len(ids)); struct.pack_into('<I', a, 102, len(ids) if count is None else count); struct.pack_into('<%dI' % len(ids), a, 106, *ids); return bytes(a)
    def test_ok(self): self.assertIsNone(R.history_bad(self._a([3, 5, 9])))
    def test_swapped(self): self.assertIn('ascending', R.history_bad(self._a([3, 9, 5])))
    def test_dup(self): self.assertIn('ascending', R.history_bad(self._a([3, 3, 5])))
    def test_count_too_big(self): self.assertIn('longer', R.history_bad(self._a([3, 5], count=99)))
    def test_real_file_clean(self):
        d = tempfile.mkdtemp(); p = os.path.join(d, 't.rte'); shutil.copy(SRC, p)
        self.assertFalse(R.stream_inventory(p)['Global/History'][1].startswith('error'))
if __name__ == '__main__': unittest.main()
