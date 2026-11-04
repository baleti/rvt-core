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


@unittest.skipUnless(SAMPLE, 'sample corpus absent')
class DamagedBasicFileInfo(unittest.TestCase):
    """the Revit year that selects decode tables comes from BasicFileInfo; a flipped year digit must not win over the Formats/Latest schema"""
    def test_flipped_year_digit_is_overruled(self):
        import tempfile, rvt_params as P
        P._YEAR_CACHE.clear()
        d = bytearray(open(SAMPLE, 'rb').read()); i = d.find('2024'.encode('utf-16le')); self.assertGreater(i, 0)
        d[i + 6] = ord('5')
        t = tempfile.NamedTemporaryFile(suffix='.rte'); t.write(d); t.flush()
        self.assertEqual(P._file_year(t.name), '2024')
        P._YEAR_CACHE.clear(); self.assertEqual(P._file_year(SAMPLE), '2024')


@unittest.skipUnless(SAMPLE, 'sample corpus absent')
class VanishedStream(unittest.TestCase):
    """a bit flip in a directory entry NAME makes the stream vanish; load() used to carry on without it"""
    def test_renamed_required_stream_is_rejected(self):
        import struct, tempfile, olefile, rvt_scan as R
        d0 = open(SAMPLE, 'rb').read(); o = olefile.OleFileIO(SAMPLE); ss = o.sectorsize
        chain = []; s = struct.unpack_from('<I', d0, 48)[0]
        while s < 0xFFFFFFFA: chain.append(s); s = o.fat[s]
        sid = [i for i, e in enumerate(o.direntries) if e is not None and e.name == 'Latest' and o.direntries[e.sid_parent if hasattr(e, 'sid_parent') else 0] is not None][-1]
        per = ss // 128; off = (chain[sid // per] + 1) * ss + (sid % per) * 128
        d = bytearray(d0); d[off] ^= 1
        t = tempfile.NamedTemporaryFile(suffix='.rte'); t.write(d); t.flush()
        self.assertRaises(ValueError, R.load, t.name)
    def test_clean_file_loads(self):
        import rvt_scan as R
        R.load(SAMPLE)



@unittest.skipUnless(SAMPLE, 'sample corpus absent')
class DirectorySizeCut(unittest.TestCase):
    """a directory entry whose size was reduced below the real chain cut Global/ElemTable silently (4096 bytes less: 892 -> 430 records); the gzip CRC/ISIZE trailer now rejects it"""
    def _cut(self, name, delta):
        import struct, tempfile, olefile
        o = olefile.OleFileIO(SAMPLE); ss = o.sectorsize
        s = o.first_dir_sector; chain = []
        while s < 0xFFFFFFFA and s not in chain: chain.append(s); s = o.fat[s]
        i = next(k for k, e in enumerate(o.direntries) if e is not None and e.entry_type == 2 and e.name == name)
        off = (chain[(i * 128) // ss] + 1) * ss + (i * 128) % ss
        d = bytearray(open(SAMPLE, 'rb').read()); sz = struct.unpack_from('<Q', d, off + 120)[0]
        struct.pack_into('<Q', d, off + 120, sz - delta)
        t = tempfile.NamedTemporaryFile(suffix='.rvt'); t.write(d); t.flush(); return t
    def test_cut_elemtable_rejected(self):
        import rvt_scan as R
        t = self._cut('ElemTable', 4096)
        self.assertRaises(ValueError, R.load, t.name)
    def test_cut_partition_rejected(self):
        import rvt_scan as R, olefile
        o = olefile.OleFileIO(SAMPLE)
        big = max((e for e in o.direntries if e is not None and e.entry_type == 2 and e.name.isdigit()), key=lambda e: e.size)
        t = self._cut(big.name, big.size // 3)
        self.assertRaises(ValueError, R.load, t.name)
    def test_fat_entry_redirected_back_inside_own_chain_rejected(self):
        import struct, tempfile, olefile, rvt_scan as R
        d0 = open(SAMPLE, 'rb').read(); o = olefile.OleFileIO(SAMPLE); ss = o.sectorsize; per = ss // 4
        big = max((e for e in o.direntries if e is not None and e.entry_type == 2), key=lambda e: e.size)
        chain = [big.isectStart]
        while len(chain) < 200: chain.append(o.fat[chain[-1]])
        sec = chain[50]; fatsec = struct.unpack_from('<I', d0, 76 + 4 * (sec // per))[0]
        d = bytearray(d0); struct.pack_into('<I', d, (fatsec + 1) * ss + 4 * (sec % per), chain[46])    # a 4-sector loop inside the chain (as the measured bit flip 5540 -> 5536)
        t = tempfile.NamedTemporaryFile(suffix='.rvt'); t.write(d); t.flush()
        self.assertRaises(ValueError, R.check_fat_consistent, olefile.OleFileIO(t.name))     # load() may reject it earlier for other reasons; the FAT check alone must see it
        R.check_fat_consistent(olefile.OleFileIO(SAMPLE))
    def test_minifat_header_damage_rejected(self):
        import tempfile, olefile, rvt_scan as R
        d = bytearray(open(SAMPLE, 'rb').read()); d[63] ^= 0x04       # high byte of the first MiniFAT sector (measured silent: 5 small streams changed)
        t = tempfile.NamedTemporaryFile(suffix='.rvt'); t.write(d); t.flush()
        self.assertRaises(ValueError, R.check_fat_consistent, olefile.OleFileIO(t.name))
    def test_clean_loads(self):
        import rvt_scan as R
        R.load(SAMPLE)
    def test_duplicate_or_renamed_stream_rejected(self):
        import struct, tempfile, olefile, rvt_scan as R
        o = olefile.OleFileIO(SAMPLE); ss = o.sectorsize
        s = o.first_dir_sector; chain = []
        while s < 0xFFFFFFFA and s not in chain: chain.append(s); s = o.fat[s]
        ents = [k for k, e in enumerate(o.direntries) if e is not None and e.entry_type == 2]
        off = lambda i: (chain[(i * 128) // ss] + 1) * ss + (i * 128) % ss
        for a, b in ((ents[0], ents[1]), (ents[2], ents[3])):      # entry b takes the name of entry a
            d = bytearray(open(SAMPLE, 'rb').read()); d[off(b):off(b) + 66] = d[off(a):off(a) + 66]
            t = tempfile.NamedTemporaryFile(suffix='.rvt'); t.write(d); t.flush()
            self.assertRaises(ValueError, R.load, t.name)
if __name__ == '__main__': unittest.main()
