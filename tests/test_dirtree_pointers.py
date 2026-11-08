"""A flipped CFB directory sibling/child pointer must not read as ok (hidden or duplicated entries); Autodesk sample, skipped if absent."""
import os, sys, glob, struct, tempfile, unittest
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
SRC = (glob.glob(os.path.expanduser('~/rvt-research/corpus/2024/69_*.rte')) or [None])[0]
@unittest.skipUnless(SRC, 'sample corpus absent')
class Tree(unittest.TestCase):
    def setUp(self):
        import olefile
        self.d = bytearray(open(SRC, 'rb').read()); o = olefile.OleFileIO(SRC); ss = o.sectorsize; ch = []; s = o.first_dir_sector
        while s < 0xfffffffa: ch.append(s); s = o.fat[s]
        self.offs = [(s + 1) * ss + k * 128 for s in ch for k in range(ss // 128)]
    def chk(self, d):
        import rvtreadcov as R
        f = tempfile.NamedTemporaryFile(suffix='.rte', delete=False); f.write(d); f.close(); self.addCleanup(os.unlink, f.name)
        return R.stream_inventory(f.name)
    def test_intact(self): self.chk(self.d)
    def test_hidden_entry(self):
        n = 0
        for off in self.offs:
            for fo in (68, 72, 76):
                if self.d[off + 66] in (1, 2, 5) and struct.unpack_from('<I', self.d, off + fo)[0] < 0xfffffffa:
                    e = bytearray(self.d); struct.pack_into('<I', e, off + fo, 0xffffffff); n += 1
                    with self.assertRaises(ValueError): self.chk(e)
        self.assertGreater(n, 5)
    def test_cycle(self):
        off = [o for o in self.offs if self.d[o + 66] == 2][0]; e = bytearray(self.d); struct.pack_into('<I', e, off + 68, 0)   # left sibling -> root
        with self.assertRaises(ValueError): self.chk(e)
if __name__ == '__main__': unittest.main()
