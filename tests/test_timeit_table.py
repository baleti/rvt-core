import os, sys, json, subprocess, unittest
HERE = os.path.dirname(os.path.abspath(__file__)); B = os.path.join(os.path.dirname(HERE), 'bench'); sys.path.insert(0, B)
import timeit_table as T
PY = sys.executable

class Tst(unittest.TestCase):
    def test_median(self):
        self.assertEqual(T.median([3, 1, 2]), 2); self.assertEqual(T.median([4, 1, 2, 3]), 2.5); self.assertIsNone(T.median([])); self.assertEqual(T.median([5]), 5)
    def test_run_ok(self):
        r = T.run_timed('%s -c "import time; time.sleep(0.05)"' % PY, n=3, warmup=1)
        self.assertEqual((r['n'], len(r['times']), r['fails']), (3, 3, 0)); self.assertGreaterEqual(r['min'], 0.04); self.assertLessEqual(r['min'], r['median']); self.assertLessEqual(r['median'], r['max'])
        self.assertEqual(r['min'], round(min(r['times']), 3))
    def test_run_fail_and_timeout(self):
        r = T.run_timed('%s -c "import sys; sys.exit(3)"' % PY, n=2); self.assertEqual((r['fails'], r['times'], r['min'], r['median']), (2, [], None, None))
        r = T.run_timed('%s -c "import time; time.sleep(5)"' % PY, n=1, timeout=0.3); self.assertEqual((r['fails'], r['times']), (1, []))
    def test_warmup_not_counted_and_env(self):
        r = T.run_timed('%s -c "import os,sys; sys.exit(0 if os.environ.get(\'TT_X\')==\'1\' else 1)"' % PY, n=2, env={'TT_X': '1'}); self.assertEqual(r['fails'], 0)
    def test_parse_spec(self):
        self.assertEqual(T.parse_spec('cold=python x.py --a=b'), ('cold', 'python x.py --a=b')); self.assertEqual(T.parse_spec('echo hi'), ('echo hi', 'echo hi'))
        self.assertEqual(T.parse_spec('FOO=1 python x.py')[0], 'FOO'); n, c = T.parse_spec('a b=c'); self.assertEqual((n, c), ('a b=c', 'a b=c'))
        n, c = T.parse_spec('x' * 60); self.assertEqual((len(n), c), (40, 'x' * 60))
    def test_format(self):
        ok = {'cmd': 'c', 'n': 2, 'times': [0.1, 0.3], 'fails': 0, 'min': 0.1, 'median': 0.2, 'mean': 0.2, 'max': 0.3}
        bad = {'cmd': 'd', 'n': 2, 'times': [], 'fails': 2, 'min': None, 'median': None, 'mean': None, 'max': None}
        md = T.format_md([('a', ok), ('b', bad)]).splitlines()
        self.assertEqual(md[0], '| name | runs | min | median | mean | max | fails |'); self.assertTrue(set(md[1]) <= set('|- :'))
        self.assertEqual(md[2], '| a | 2 | 0.100 | 0.200 | 0.200 | 0.300 | 0 |'); self.assertEqual(md[3], '| b | 0 | n/a | n/a | n/a | n/a | 2 |')
        j = json.loads(T.format_json([('a', ok)])); self.assertEqual(j['a']['median'], 0.2)
    def test_cli(self):
        cli = [PY, os.path.join(B, 'timeit_table.py')]
        p = subprocess.run(cli + ['-n', '2', 'fast=%s -c "pass"' % PY], capture_output=True, text=True); self.assertEqual(p.returncode, 0); self.assertIn('| fast | 2 |', p.stdout)
        p = subprocess.run(cli + ['-n', '1', '--json', 'bad=%s -c "raise SystemExit(1)"' % PY], capture_output=True, text=True); self.assertEqual(p.returncode, 1); self.assertEqual(json.loads(p.stdout)['bad']['fails'], 1)
        p = subprocess.run(cli + ['-n', '1', '-e', 'TT_Y=ok', 'e=%s -c "import os,sys; sys.exit(0 if os.environ.get(\'TT_Y\')==\'ok\' else 1)"' % PY], capture_output=True, text=True); self.assertEqual(p.returncode, 0)
if __name__ == '__main__': unittest.main()
