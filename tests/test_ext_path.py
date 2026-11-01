"""rvt_fast finds the extension under every platform's file name and falls back (None) when absent. Synthetic: touches empty files only."""
import os, sys, tempfile, shutil, importlib.util, unittest
SRC = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'pysrc', 'rvt_fast.py')

def load(d):
    shutil.copy(SRC, d); spec = importlib.util.spec_from_file_location('rvt_fast_t', os.path.join(d, 'rvt_fast.py'))
    os.environ['RVT_NOFAST'] = '1'; m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m

class T(unittest.TestCase):
    def test_names(self):
        for name in ('rvt_core.abi3.so', 'rvt_core.pyd', 'rvt_core.cp39-win_amd64.pyd', 'rvt_core.abi3.pyd'):
            d = tempfile.mkdtemp(); self.addCleanup(shutil.rmtree, d, True); m = load(d)
            self.assertIsNone(m._ext_path()); open(os.path.join(d, name), 'wb').close()
            self.assertEqual(os.path.basename(m._ext_path()), name)
if __name__ == '__main__': unittest.main()
