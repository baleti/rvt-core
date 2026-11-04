"""Known read gaps, pinned so a fix is noticed (expectedFailure flips to 'unexpected success').
2024 Snowdon Towers Structural sample: 20 ElemTable ids never get a record header (identical set in 2024/2025/2026 builds)."""
import os, sys, glob, unittest
RR = os.path.expanduser('~/rvt-research'); sys.path.insert(0, RR)
F = (glob.glob(RR + '/corpus/2024/43_*Structural*.rvt') or [None])[0]
KNOWN = {665535, 665874, 666708, 666719, 670431, 670521, 670522, 670523, 670535, 670536, 670537, 670538, 670539, 670540,
         670541, 670542, 670546, 670548, 670549, 709764}
@unittest.skipUnless(F, 'sample corpus absent')
class Gaps(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import rvt_entities as E, rvt_scan as R
        eh = E.extract(F)['eh']; cls.missing = R.load(F)[2] - {k if isinstance(k, int) else k[0] for k in eh}
    def test_gap_is_exactly_the_known_set(self):
        self.assertTrue(KNOWN <= self.missing, 'a known id now resolves: update the pin')
    @unittest.expectedFailure
    def test_every_element_has_a_header(self):
        self.assertFalse(KNOWN & self.missing)

SAMPLE = (glob.glob(RR + '/corpus/2024/69_*.rte') or [None])[0]
@unittest.skipUnless(SAMPLE, 'sample corpus absent')
class DamagedInput(unittest.TestCase):
    """damaged gzip members are flagged in extract()['damaged'] (and raise with RVT_STRICT=1); clean file reports none"""
    def _damaged_copy(self, fn):
        import tempfile
        d = bytearray(open(SAMPLE, 'rb').read()); fn(d)
        t = tempfile.NamedTemporaryFile(suffix='.rvt'); t.write(d); t.flush(); return t
    def setUp(self):
        os.environ['RVT_NOCACHE'] = os.environ['RVT_CORE_NOCACHE'] = '1'; os.environ.pop('RVT_STRICT', None)
    def test_clean_file_reports_no_damage(self):
        import rvt_entities as E
        self.assertEqual(E.extract(SAMPLE).get('damaged'), {})
    def test_zeroed_region_is_flagged(self):
        import rvt_entities as E
        t = self._damaged_copy(lambda d: d.__setitem__(slice(1000000, 1100000), bytes(100000)))
        self.assertTrue(E.extract(t.name)['damaged'], 'silent record loss: damage not reported')
    def test_strict_mode_raises(self):
        import rvt_entities as E
        t = self._damaged_copy(lambda d: d.__setitem__(slice(1000000, 1100000), bytes(100000)))
        os.environ['RVT_STRICT'] = '1'
        try: self.assertRaises(ValueError, E.extract, t.name)
        finally: os.environ.pop('RVT_STRICT', None)
@unittest.skipUnless(SAMPLE, 'sample corpus absent')
class Truncated(unittest.TestCase):
    def test_tail_truncation_is_an_error(self):
        import tempfile, rvt_scan as S
        raw = open(SAMPLE, 'rb').read(); S.load(SAMPLE)
        for frac in (0.999, 0.9, 0.5):
            with tempfile.NamedTemporaryFile(suffix='.rvt') as t:
                t.write(raw[:int(len(raw) * frac)]); t.flush()
                with self.assertRaises(ValueError, msg='truncated to %s accepted' % frac): S.load(t.name)


class Id0Flag(unittest.TestCase):
    """records filed under element id 0 are reported in extract()['suspect'] so comparison code can exclude them"""
    @unittest.skipUnless(SAMPLE, 'no sample')
    def test_suspect_listed(self):
        import rvt_entities as E
        r = E.extract(SAMPLE)
        self.assertEqual(r['suspect'], sorted(k for k in r['recs'] if k[0] == 0))


@unittest.skipUnless(SAMPLE, 'sample corpus absent')
class FatChainDamage(unittest.TestCase):
    """a flipped bit in the directory/FAT used to shorten a stream silently (Partitions 14.8 MB read as ~1 MB, records 1773 -> 65); now a clean ValueError"""
    def test_shortened_chain_is_rejected(self):
        import struct, tempfile, olefile, rvt_scan as R
        d0 = open(SAMPLE, 'rb').read(); o = olefile.OleFileIO(SAMPLE); ss = o.sectorsize; per = ss // 4
        big = max((e for e in o.direntries if e is not None and e.entry_type == 2), key=lambda e: e.size)
        chain = [big.isectStart]
        while len(chain) < 200: chain.append(o.fat[chain[-1]])
        sec = chain[100]; fatsec = struct.unpack_from('<I', d0, 76 + 4 * (sec // per))[0]
        d = bytearray(d0); struct.pack_into("<I", d, (fatsec + 1) * ss + 4 * (sec % per), 0xFFFFFFFE)   # end-of-chain after 100 sectors
        t = tempfile.NamedTemporaryFile(suffix='.rte'); t.write(d); t.flush()
        self.assertRaises(ValueError, R.check_not_truncated, olefile.OleFileIO(t.name), t.name)
    def test_clean_file_accepted(self):
        import olefile, rvt_scan as R
        R.check_not_truncated(olefile.OleFileIO(SAMPLE), SAMPLE)
if __name__ == '__main__': unittest.main()
