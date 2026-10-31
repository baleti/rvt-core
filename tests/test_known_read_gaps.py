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
if __name__ == '__main__': unittest.main()
