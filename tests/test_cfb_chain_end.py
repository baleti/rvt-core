"""The last FAT entry of a stream chain set to free / FAT-sector (instead of ENDOFCHAIN) was read ok because the reader stops at the stream size; must be an error. Autodesk sample, skipped if absent."""
import os, sys, glob, struct, tempfile, unittest
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
SRC = (glob.glob(os.path.expanduser('~/rvt-research/corpus/2024/69_*.rte')) or [None])[0]
@unittest.skipUnless(SRC, 'sample corpus absent')
class ChainEnd(unittest.TestCase):
    def chk(self, d):
        import rvtreadcov as R
        f = tempfile.NamedTemporaryFile(suffix='.rte', delete=False); f.write(d); f.close(); self.addCleanup(os.unlink, f.name)
        return R.stream_inventory(f.name)
    def test_chain_end_flip(self):
        import olefile
        raw = open(SRC, 'rb').read(); o = olefile.OleFileIO(SRC); ss = o.sectorsize
        nfat = struct.unpack_from('<I', raw, 44)[0]; fats = [struct.unpack_from('<I', raw, 76 + 4 * i)[0] for i in range(nfat)]
        self.chk(raw)
        n = 0
        for p in o.listdir(streams=True):
            e = o.direntries[o._find('/'.join(p))]
            if e.size < o.minisectorcutoff: continue
            s = e.isectStart; last = s
            while s < 0xfffffffa: last = s; s = o.fat[s]
            for val in (0xffffffff, 0xfffffffd):
                b = bytearray(raw); struct.pack_into('<I', b, (fats[last // (ss // 4)] + 1) * ss + 4 * (last % (ss // 4)), val); n += 1
                with self.assertRaises(ValueError): self.chk(b)
        self.assertGreater(n, 4)
if __name__ == '__main__': unittest.main()
