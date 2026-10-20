import os, sys, tempfile, subprocess, unittest
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
import suite_report as S

L_IDENT = 'alpha.rvt     index  34011 (0 differ) records 200 (0 differ) order True  oracle   0.9s fast  0.17s  IDENTICAL'
L_EL = 'beta.rvt      5426 elements  oracle  0.50s fast  0.29s  IDENTICAL'
L_WA = 'gamma.rvt     want_all recs   5426 eh   5425  oracle   30.9s fast   0.5s  IDENTICAL'
L_OK = 'delta.rvt     300 elements:  291 bit-identical   9 geometrically equivalent   0 DIFFERENT  oracle load  14.2s mesh    6.5s | fast load   0.7s mesh    1.2s  OK'
L_FAIL = 'epsilon.rvt   FAILED rc 137'
L_DIFF = 'zeta.rvt      300 elements:  290 bit-identical   8 geometrically equivalent   2 DIFFERENT  oracle load  1.0s mesh 2.0s | fast load 0.5s mesh 0.5s'

class T(unittest.TestCase):
    def test_parse(self):
        p = S.parse_line(L_IDENT); self.assertEqual((p['name'], p['status']), ('alpha.rvt', 'ok')); self.assertAlmostEqual(p['oracle_s'], 0.9); self.assertAlmostEqual(p['fast_s'], 0.17)
        p = S.parse_line(L_WA); self.assertAlmostEqual(p['oracle_s'], 30.9); self.assertAlmostEqual(p['fast_s'], 0.5)
        p = S.parse_line(L_OK); self.assertEqual(p['status'], 'ok'); self.assertAlmostEqual(p['oracle_s'], 20.7); self.assertAlmostEqual(p['fast_s'], 1.9)
        p = S.parse_line(L_FAIL); self.assertEqual(p['status'], 'failed'); self.assertIsNone(p['oracle_s']); self.assertIsNone(p['fast_s'])
        p = S.parse_line(L_DIFF); self.assertEqual(p['status'], 'different'); self.assertAlmostEqual(p['oracle_s'], 3.0); self.assertAlmostEqual(p['fast_s'], 1.0)
        self.assertEqual(S.parse_line('x.rvt 5 elements oracle 1.0s fast 0.5s DIFFERENT')['status'], 'different')
        for ign in ('', '   ', '# comment', '== header'): self.assertIsNone(S.parse_line(ign))
    def test_summarise_last_wins(self):
        s = S.summarise([L_EL, L_FAIL, L_DIFF, L_OK, L_EL.replace('0.29s', '0.25s'), 'epsilon.rvt 10 elements oracle 1.0s fast 0.5s IDENTICAL'])
        self.assertEqual((s['files'], s['ok'], s['failed'], s['different']), (4, 3, 0, 1))
        self.assertAlmostEqual(s['oracle_s'], 0.5 + 20.7 + 3.0 + 1.0); self.assertAlmostEqual(s['fast_s'], 0.25 + 1.9 + 1.0 + 0.5)
        self.assertEqual(s['speedup'], round(s['oracle_s'] / s['fast_s'], 1))
    def test_summarise_no_timing(self):
        s = S.summarise([L_FAIL]); self.assertEqual((s['files'], s['failed']), (1, 1)); self.assertIsNone(s['speedup'])
    def test_suite_name(self):
        self.assertEqual(S.suite_name('/x/h1_f7_mesh.log', 'f7_'), 'mesh'); self.assertEqual(S.suite_name('/x/h1_f7_mesh.log'), 'f7_mesh'); self.assertEqual(S.suite_name('h1_params.log'), 'params')
    def test_report_and_cli(self):
        with tempfile.TemporaryDirectory() as d:
            a = os.path.join(d, 'h1_f7_g103.log'); b = os.path.join(d, 'h1_f7_mesh.log'); e = os.path.join(d, 'h1_f7_mesh.log.err'); sm = os.path.join(d, 'h1_summary.log')
            open(a, 'w').write(L_IDENT + '\n' + L_EL + '\n'); open(b, 'w').write(L_OK + '\n' + L_DIFF + '\n'); open(e, 'w').write('noise\n'); open(sm, 'w').write('f7_g103 done 1: 2 ok\n')
            r = S.report([a, b, e, os.path.join(d, 'h1_f7_missing.log')], 'f7_').splitlines()
            self.assertEqual(r[0], '| suite | files | ok | failed | DIFFERENT | oracle s | fast s | speedup |')
            self.assertTrue(r[1].startswith('|') and set(r[1]) <= set('|- :'))
            self.assertEqual(r[2], '| g103 | 2 | 2 | 0 | 0 | 1.4 | 0.5 | 3.0x |')
            self.assertEqual(r[3], '| mesh | 2 | 1 | 0 | 1 | 23.7 | 2.9 | 8.2x |')
            self.assertEqual(r[4], '| missing | n/a | n/a | n/a | n/a | n/a | n/a | n/a |'); self.assertEqual(len(r), 5)
            cli = [sys.executable, os.path.join(HERE, 'suite_report.py')]
            p = subprocess.run(cli + ['--prefix', 'f7_', d], capture_output=True, text=True)
            self.assertEqual(p.returncode, 1); out = p.stdout.splitlines()
            self.assertEqual([x.split('|')[1].strip() for x in out[2:]], ['g103', 'mesh']); self.assertNotIn('summary', p.stdout)
            p = subprocess.run(cli + [a], capture_output=True, text=True); self.assertEqual(p.returncode, 0)
if __name__ == '__main__': unittest.main()
