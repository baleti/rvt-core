import os, sys, tempfile
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'pysrc'))
import rvt_fast as R


def test_same_size_replace_with_restored_mtime_changes_the_cache_key(tmp_path):
    """a SAVE! (rename-replace, same size, same-second or restored mtime) must not reuse the previous version's cache"""
    p = str(tmp_path / 'a.rvt'); open(p, 'wb').write(b'A' * 100); k1 = R.file_key(p); st = os.stat(p)
    open(p + '.n', 'wb').write(b'B' * 100); os.replace(p + '.n', p); os.utime(p, ns=(st.st_atime_ns, st.st_mtime_ns))
    assert R.file_key(p) != k1


def test_subsecond_rewrite_changes_the_key(tmp_path):
    p = str(tmp_path / 'a.rvt'); open(p, 'wb').write(b'A' * 100); k1 = R.file_key(p); st = os.stat(p)
    open(p, 'wb').write(b'B' * 100); os.utime(p, ns=(st.st_atime_ns, st.st_mtime_ns + 5_000_000))
    assert R.file_key(p) != k1
