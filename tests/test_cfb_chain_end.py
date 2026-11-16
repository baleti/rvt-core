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
    def test_mini_chain_end_flip(self):
        import olefile
        raw = open(SRC, 'rb').read(); o = olefile.OleFileIO(SRC); ss = o.sectorsize; o.loadminifat()
        nfat = struct.unpack_from('<I', raw, 44)[0]; fats = [struct.unpack_from('<I', raw, 76 + 4 * i)[0] for i in range(nfat)]
        mf = []; s = o.first_mini_fat_sector
        while s < 0xfffffffa: mf.append(s); s = o.fat[s]
        n = 0
        for p in o.listdir(streams=True):
            e = o.direntries[o._find('/'.join(p))]
            if e.size >= o.minisectorcutoff or e.size == 0: continue
            s = e.isectStart; last = s
            while s < 0xfffffffa: last = s; s = o.minifat[s]
            for val in (0xffffffff, 0xfffffffd):
                b = bytearray(raw); struct.pack_into('<I', b, (mf[last // (ss // 4)] + 1) * ss + 4 * (last % (ss // 4)), val); n += 1
                with self.assertRaises(ValueError): self.chk(b)
        self.assertGreater(n, 4)
    def test_start_pointer_into_other_chain(self):
        import olefile
        S71 = (glob.glob(os.path.expanduser('~/rvt-research/corpus/2024/71_*.rte')) or [None])[0]
        if not S71: self.skipTest('71 sample absent')
        raw = open(S71, 'rb').read(); o = olefile.OleFileIO(S71); ss = o.sectorsize
        ds = []; s = o.first_dir_sector
        while s < 0xfffffffa: ds.append(s); s = o.fat[s]
        eo = lambda sid: (ds[sid // (ss // 128)] + 1) * ss + (sid % (ss // 128)) * 128
        e1 = o._find(['Contents']); e2 = o._find(next(x for x in o.listdir() if x[0] == 'Partitions'))
        b = bytearray(raw); struct.pack_into('<I', b, eo(e1) + 116, o.direntries[e2].isectStart)
        with self.assertRaises(ValueError): self.chk(b)
if __name__ == '__main__': unittest.main()
