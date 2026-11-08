"""check_members must reject a gzip member whose 8-byte trailer is cut short (stream-size / chain damage); synthetic data only."""
import os, sys, gzip, unittest
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvtreadcov as R
class Trailer(unittest.TestCase):
    def test_intact(self): self.assertEqual(R.check_members(gzip.compress(b'x' * 5000)), 1)
    def test_trailer_cut(self):
        g = gzip.compress(b'x' * 5000)
        for k in (1, 4, 7): self.assertRaises(ValueError, R.check_members, g[:-k])
    def test_trailing_slack_ok(self): self.assertEqual(R.check_members(gzip.compress(b'y' * 900) + b'\0' * 37), 1)
if __name__ == '__main__': unittest.main()
