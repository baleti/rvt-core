import os, sys, json, tempfile, subprocess, unittest
HERE = os.path.dirname(os.path.abspath(__file__)); TOOLS = os.path.join(os.path.dirname(HERE), 'tools'); sys.path.insert(0, TOOLS)
import progress_tail as P

def rec(**kw):
    d = {'file': '/a/b/model.rvt', 'stage': 'read', 'done': 5, 'total': 10, 'elements_decoded': 0, 'meshes_done': 0, 'meshes_total': 0, 'bytes_read': 2 * 1024 * 1024, 'eta_s': 12.0, 'started': 100.0, 'updated': 110.0, 'final': False}
    d.update(kw); return d

class T(unittest.TestCase):
    def test_bar(self):
        self.assertEqual(P.bar(5, 10), '[#####-----]'); self.assertEqual(P.bar(0, 10), '[----------]'); self.assertEqual(P.bar(10, 10), '[##########]')
        self.assertEqual(P.bar(99, 10), '[##########]'); self.assertEqual(P.bar(3, 0), '[----------]'); self.assertEqual(P.bar(1, 3, 6), '[##----]'); self.assertEqual(P.bar(-4, 10), '[----------]')
    def test_bytes(self):
        self.assertEqual([P.fmt_bytes(x) for x in (512, 1536, 2 * 1024 * 1024, int(3.2 * 1024 ** 3), None, -5)], ['512 B', '1.5 KB', '2.0 MB', '3.2 GB', '0 B', '0 B'])
    def test_eta(self):
        self.assertEqual([P.fmt_eta(x) for x in (None, 12.0, 0.04, 65, 3720)], ['?', '12.0s', '0.0s', '1m05s', '1h02m'])
    def test_render(self):
        s = P.render(rec(), 115.0)
        self.assertEqual(s, 'model.rvt                     read        [#####-----]  50%  eta 12.0s  2.0 MB')
        s = P.render(rec(final=True, stage='done', done=10, eta_s=0.0), 500.0)
        self.assertEqual(s, 'model.rvt                     done        [##########]  100%  eta done  2.0 MB')
        s = P.render(rec(), 155.0); self.assertTrue(s.endswith('  (stale 45s)'))
        self.assertEqual(P.render(rec(total=0, done=0, eta_s=None), 111.0), 'model.rvt                     read        [----------]  0%  eta ?  2.0 MB')
        self.assertEqual(len(P.render(rec(file='/x/' + 'n' * 60 + '.rvt'), 111.0)), len(P.render(rec(), 111.0)))
        self.assertEqual(len(P.render(rec(), 155.0, width=40)), 40)
    def test_load_all_and_cli(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(P.load_all(os.path.join(d, 'nope')), [])
            json.dump(rec(file='/a/old.rvt', updated=1.0), open(os.path.join(d, 'k1.json'), 'w')); json.dump(rec(file='/a/new.rvt', updated=9.0, final=True), open(os.path.join(d, 'k2.json'), 'w'))
            open(os.path.join(d, 'bad.json'), 'w').write('{not json'); open(os.path.join(d, 'k3.json.1.2.tmp'), 'w').write('{}'); json.dump([1], open(os.path.join(d, 'list.json'), 'w'))
            self.assertEqual([x['file'] for x in P.load_all(d)], ['/a/new.rvt', '/a/old.rvt'])
            cli = [sys.executable, os.path.join(TOOLS, 'progress_tail.py'), '--dir', d, '--once']
            p = subprocess.run(cli, capture_output=True, text=True); self.assertEqual(p.returncode, 0); self.assertEqual(len(p.stdout.strip().splitlines()), 2); self.assertTrue(p.stdout.startswith('new.rvt'))
            p = subprocess.run(cli + ['--active'], capture_output=True, text=True); self.assertEqual(len(p.stdout.strip().splitlines()), 1); self.assertTrue(p.stdout.startswith('old.rvt'))
            p = subprocess.run([sys.executable, os.path.join(TOOLS, 'progress_tail.py'), '--dir', os.path.join(d, 'none'), '--once'], capture_output=True, text=True); self.assertEqual(p.stdout.strip(), 'no progress files')
if __name__ == '__main__': unittest.main()
