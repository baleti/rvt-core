"""member_diff: a rewrite that touches one member must leave the others compressed-byte identical; synthetic data only."""
import os, sys, zlib, struct, unittest
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
import rvtreadcov as R
def gz(data, level):
    c = zlib.compressobj(level, zlib.DEFLATED, -15); body = c.compress(data) + c.flush()
    return b'\x1f\x8b\x08\x00\0\0\0\0\x00\xff' + body + struct.pack('<II', zlib.crc32(data) & 0xffffffff, len(data))
PL = [bytes((i * 7 + j) % 251 for j in range(4000)) for i in range(5)]
def stream(pl, lv=9, over=None): return b''.join(gz(p, (over or {}).get(i, lv)) for i, p in enumerate(pl))
class MemberDiff(unittest.TestCase):
    def test_identical(self): self.assertEqual(R.member_diff(stream(PL), stream(PL)), (0, 0, 5))
    def test_one_edited(self):
        pl = list(PL); pl[2] = pl[2][:10] + b'ZZ' + pl[2][12:]
        self.assertEqual(R.member_diff(stream(PL), stream(pl)), (1, 1, 5))
    def test_recompressed_same_plaintext(self):   # equivalent but not identical: must be visible as comp differ with plain equal
        self.assertEqual(R.member_diff(stream(PL), stream(PL, over={3: 1})), (1, 0, 5))
    def test_count_mismatch(self): self.assertRaises(ValueError, R.member_diff, stream(PL), stream(PL[:4]))
if __name__ == '__main__': unittest.main()
