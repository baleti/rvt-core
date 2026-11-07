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
if __name__ == '__main__': unittest.main()
