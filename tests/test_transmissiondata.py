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
