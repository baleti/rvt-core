"""Clear-error behaviour of tools/rvtreadcov.py on hostile inputs derived from an Autodesk sample (skipped if the corpus is absent)."""
import os, sys, glob, subprocess, json, tempfile, unittest
HERE = os.path.dirname(os.path.abspath(__file__)); TOOL = os.path.join(HERE, '..', 'tools', 'rvtreadcov.py')
SRC = (glob.glob(os.path.expanduser('~/rvt-research/corpus/2024/69_*.rte')) or [None])[0]
def run(p):
    return json.loads(subprocess.run([sys.executable, TOOL, p], capture_output=True, text=True).stdout.strip().splitlines()[-1])
@unittest.skipUnless(SRC, 'sample corpus absent')
class Hostile(unittest.TestCase):
    def mk(self, data):
        f = tempfile.NamedTemporaryFile(suffix='.rvt', delete=False); f.write(data); f.close(); self.addCleanup(os.unlink, f.name); return f.name
    def test_intact(self): self.assertEqual(run(SRC)['open'], 'ok')
    def test_zero(self): self.assertTrue(run(self.mk(b''))['open'].startswith('error:'))
    def test_random(self): self.assertTrue(run(self.mk(os.urandom(200000)))['open'].startswith('error:'))
    def test_head_truncated(self): self.assertTrue(run(self.mk(open(SRC, 'rb').read(100000)))['open'].startswith('error:'))
    def test_tail_truncated(self):
        n = os.path.getsize(SRC); r = run(self.mk(open(SRC, 'rb').read(n - 700000)))
        self.assertIn('truncated', r['open'])   # olefile zero-pads; must not read as ok
    def test_bitflip_in_partition(self):
        import olefile
        f = self.mk(open(SRC, 'rb').read()); o = olefile.OleFileIO(f, write_mode=True)
        n = [('/'.join(e)) for e in o.listdir(streams=True) if e[0] == 'Partitions'][0]
        import random; random.seed(1); d = bytearray(o.openstream(n).read())
        for _ in range(5): d[random.randrange(len(d) // 4, len(d) * 3 // 4)] ^= 0xff
        o.write_stream(n, bytes(d)); o.close()
        r = run(f); self.assertIn('CRC', r['open'] + str(r.get('streams')))   # corrupted deflate data must not read as decoded
    def test_bitflip_in_elemtable(self):
        import olefile, random
        f = self.mk(open(SRC, 'rb').read()); o = olefile.OleFileIO(f, write_mode=True)
        d = bytearray(o.openstream('Global/ElemTable').read()); random.seed(3)
        for _ in range(3): d[random.randrange(len(d) // 3, len(d) * 2 // 3)] ^= 0xff
        o.write_stream('Global/ElemTable', bytes(d)); o.close()
        self.assertIn('CRC', str(run(f).get('streams')))
    def _elem(self, fn):
        import olefile
        f = self.mk(open(SRC, 'rb').read()); o = olefile.OleFileIO(f, write_mode=True)
        d = o.openstream('Global/ElemTable').read(); nd = fn(bytearray(d)); assert len(nd) == len(d)
        o.write_stream('Global/ElemTable', bytes(nd)); o.close(); return run(f)
    def test_elemtable_header_zeroed(self):
        r = self._elem(lambda d: (d.__setitem__(slice(0, 64), bytes(64)), d)[1])
        self.assertNotEqual((r['open'], str(r.get('streams', {}).get('Global/ElemTable'))), ('ok', "[%s, 'decoded']" % r.get('streams', {}).get('Global/ElemTable', [0])[0]))
    def test_elemtable_all_zero(self):
        r = self._elem(lambda d: bytearray(len(d)))
        self.assertNotIn('decoded', str(r.get('streams', {}).get('Global/ElemTable')))
    def test_partitiontable_damaged(self):
        import olefile
        for mode in ('zero', 'flip'):
            f = self.mk(open(SRC, 'rb').read()); o = olefile.OleFileIO(f, write_mode=True)
            d = bytearray(o.openstream('Global/PartitionTable').read())
            if mode == 'zero': d = bytearray(len(d))
            else:
                for i in range(len(d)): d[i] ^= 0xff
            o.write_stream('Global/PartitionTable', bytes(d)); o.close()
            r = run(f)   # must be a clear error or must not claim 'ok' with a decoded PartitionTable
            self.assertTrue(r['open'] != 'ok' or 'decoded' not in str(r.get('streams', {}).get('Global/PartitionTable')), (mode, r['open']))
@unittest.skipUnless(SRC, 'sample corpus absent')
class HeaderFlip(unittest.TestCase):
    def flip(self, off, val):
        d = bytearray(open(SRC, 'rb').read()); d[off:off + len(val)] = val
        f = tempfile.NamedTemporaryFile(suffix='.rvt', delete=False); f.write(d); f.close(); self.addCleanup(os.unlink, f.name); return run(f.name)
    def test_first_minifat_wrong_chain(self): self.assertIn('mini-FAT', self.flip(60, bytes([0xfd])).get('open', ''))   # Revit 2024 refuses
    def test_minifat_count_absurd(self): self.assertIn('mini-FAT', self.flip(66, bytes([0xff])).get('open', ''))     # Revit 2024 refuses
    def test_minifat_count_larger_opens(self): self.assertEqual(self.flip(64, bytes([0xfe]))['open'], 'ok')           # Revit 2024 opens
@unittest.skipUnless(SRC, 'sample corpus absent')
class DirEntryFlip(unittest.TestCase):
    def test_storage_type_flip_drops_streams(self):
        import struct
        b = bytearray(open(SRC, 'rb').read()); ds = struct.unpack_from('<I', b, 48)[0]; base = (ds + 1) * (1 << struct.unpack_from('<H', b, 30)[0])
        hit = 0
        for e in range(1, 12):     # flip the type byte of each storage entry: every one that hides a core storage must be an error
            r = b[base + 128 * e:base + 128 * e + 128]
            if len(r) < 128 or r[66] != 1: continue
            nm = r[:64].decode('utf-16le', 'ignore').split('\0')[0]
            if nm not in ('Formats', 'Global', 'Partitions'): continue
            x = bytearray(b); x[base + 128 * e + 66] ^= 0xff
            f = tempfile.NamedTemporaryFile(suffix='.rvt', delete=False); f.write(x); f.close(); self.addCleanup(os.unlink, f.name)
            self.assertIn('missing core stream', run(f.name)['open'], nm); hit += 1
        self.assertEqual(hit, 3)
@unittest.skipUnless(SRC, 'sample corpus absent')
class FatFlip(unittest.TestCase):
    def test_chain_pointer_flip_in_partition(self):
        import olefile, struct
        b = open(SRC, 'rb').read(); o = olefile.OleFileIO(SRC); ss = 1 << struct.unpack_from('<H', b, 30)[0]
        per = ss // 4
        ent = max((o.direntries[o._find(n)] for n in ['/'.join(e) for e in o.listdir(streams=True, storages=False) if e[0] == 'Partitions']), key=lambda d: d.size)
        chain = []; s = ent.isectStart
        while s < 0xfffffffa: chain.append(s); s = o.fat[s]
        refused = 0
        for k in (len(chain) // 5, len(chain) // 2, len(chain) * 4 // 5):
            fs = struct.unpack_from('<I', b, 76 + 4 * (chain[k] // per))[0]      # FAT sector holding this entry (header DIFAT, first 109)
            x = bytearray(b); x[(fs + 1) * ss + 4 * (chain[k] % per)] ^= 1      # next-sector pointer off by one: a cut-off deflate stream used to inflate without error and skip the CRC check
            f = tempfile.NamedTemporaryFile(suffix='.rvt', delete=False); f.write(x); f.close(); self.addCleanup(os.unlink, f.name)
            r = run(f.name); refused += r['open'] != 'ok' or any(str(v[1]).startswith('error') for v in r['streams'].values())
        self.assertEqual(refused, 3)
@unittest.skipUnless(SRC, 'sample corpus absent')
class MiniFatFlip(unittest.TestCase):
    def test_minifat_pointer_flips_in_basicfileinfo(self):
        import olefile, struct, io
        b = open(SRC, 'rb').read(); o = olefile.OleFileIO(SRC); o.openstream('BasicFileInfo'); o.loadminifat(); ss = 1 << struct.unpack_from('<H', b, 30)[0]
        d = o.direntries[o._find('BasicFileInfo')]; ch = []; s = d.isectStart
        while s < 0xfffffffa: ch.append(s); s = o.minifat[s]
        caught = 0
        for k in range(0, len(ch) - 1, 3):       # mini-FAT next pointer off by one garbles the stream; only damage that removes a label is detectable (about a third of the pointers), the rest alters values
            x = bytearray(b); x[(o.first_mini_fat_sector + 1) * ss + 4 * ch[k]] ^= 1
            f = tempfile.NamedTemporaryFile(suffix='.rvt', delete=False); f.write(x); f.close(); self.addCleanup(os.unlink, f.name)
            r = run(f.name); caught += str(r['streams']['BasicFileInfo'][1]).startswith('error')
        self.assertGreaterEqual(caught, 3)
class ElemDups(unittest.TestCase):
    def tbl(self, ids, rw=28):
        import struct
        return b'\0\0' + struct.pack('<I', len(ids)) + b''.join(i.to_bytes(4, 'little') + bytes(rw - 4) for i in ids)
    def test_dups(self):
        sys.path.insert(0, os.path.join(HERE, '..', 'tools')); import rvtreadcov as R
        self.assertEqual(R.elemtable_dups(self.tbl([1, 2, 3])), 0)
        self.assertEqual(R.elemtable_dups(self.tbl([1, 2, 2, 3, 3])), 2)
        self.assertIsNone(R.elemtable_dups(b'\0\0\5\0\0\0'))
if __name__ == '__main__': unittest.main()
