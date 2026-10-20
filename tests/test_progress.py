"""Standard-library tests for progress reporting; run directly with python3."""

import json
import os
from pathlib import Path
import queue
import shutil
import sys
import tempfile
import threading
import time
import unittest


sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "pysrc"))
os.environ["RVT_NOFAST"] = "1"
from rvt_fast import Progress, file_key


class ProgressTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.mkdtemp(prefix="rvt-progress-test-")
        self.addCleanup(shutil.rmtree, self.directory)
        model_dir = tempfile.mkdtemp(prefix="rvt-model-test-")
        self.addCleanup(shutil.rmtree, model_dir)
        self.model = Path(model_dir) / "model.rvt"
        self.model.write_bytes(b"small model")
        original_dir = Progress.DIR
        self.addCleanup(setattr, Progress, "DIR", original_dir)
        Progress.DIR = self.directory

    def progress(self):
        progress = Progress(str(self.model))

        def cleanup():
            progress.close()
            progress._th.join(timeout=1)

        self.addCleanup(cleanup)
        return progress

    def read_state(self, progress):
        with open(progress.path, encoding="utf-8") as stream:
            return json.load(stream)

    def test_file_key_format_stability_and_path_spelling(self):
        key = file_key(str(self.model))
        self.assertRegex(key, r"^[0-9a-f]{16}$")
        self.assertEqual(key, file_key(str(self.model)))
        self.assertEqual(key, file_key(os.path.relpath(self.model)))

    def test_file_key_changes_with_size(self):
        before = self.model.stat()
        key = file_key(str(self.model))
        self.model.write_bytes(b"a different size")
        # Isolate size from timestamp changes caused by writing.
        os.utime(self.model, ns=(before.st_atime_ns, before.st_mtime_ns))
        self.assertNotEqual(key, file_key(str(self.model)))

    def test_file_key_changes_with_mtime(self):
        before = self.model.stat()
        key = file_key(str(self.model))
        os.utime(self.model, ns=(before.st_atime_ns,
                                before.st_mtime_ns + 2_000_000_000))
        self.assertNotEqual(key, file_key(str(self.model)))

    def test_file_key_changes_with_absolute_path(self):
        other = self.model.with_name("other.rvt")
        shutil.copy2(self.model, other)
        self.assertEqual(self.model.stat().st_size, other.stat().st_size)
        self.assertEqual(self.model.stat().st_mtime_ns, other.stat().st_mtime_ns)
        self.assertNotEqual(file_key(str(self.model)), file_key(str(other)))

    def test_initial_state_and_directory_creation(self):
        # Remove the fresh, empty DIR to exercise creation by __init__.
        os.rmdir(self.directory)
        progress = self.progress()
        self.assertTrue(os.path.isdir(self.directory))
        self.assertEqual(progress.path, os.path.join(
            self.directory, file_key(str(self.model)) + ".json"))
        expected = dict(stage="start", done=0, total=0, final=False,
                        eta_s=None, file=os.path.abspath(self.model))
        for key, value in expected.items():
            with self.subTest(key=key):
                self.assertEqual(progress.d[key], value)

    def test_stage_change_writes_immediately(self):
        progress = self.progress()
        progress.set(done=3, total=8)
        progress.set("read", total=10)
        self.assertEqual(progress.d["stage"], "read")
        self.assertEqual(progress.d["done"], 0)
        self.assertEqual(progress.d["total"], 10)
        state = self.read_state(progress)
        self.assertEqual(state["stage"], "read")
        self.assertEqual(state["done"], 0)
        self.assertEqual(state["total"], 10)
        self.assertIsInstance(state["updated"], float)

    def test_updates_keep_stage_and_eta_cases(self):
        progress = self.progress()
        progress.set("read", total=10)
        progress.set(done=4)
        self.assertEqual(progress.d["stage"], "read")
        self.assertEqual(progress.d["done"], 4)
        self.assertEqual(progress.d["total"], 10)
        self.assertIsInstance(progress.d["eta_s"], float)
        self.assertGreaterEqual(progress.d["eta_s"], 0.0)
        progress.set(done=10)
        self.assertEqual(progress.d["eta_s"], 0.0)
        self.assertIsInstance(progress.d["eta_s"], float)
        progress.set(done=0)
        self.assertIsNone(progress.d["eta_s"])
        progress.set(done=4, total=0)
        self.assertIsNone(progress.d["eta_s"])

    def test_new_stage_resets_counts(self):
        progress = self.progress()
        progress.set("read", total=10)
        progress.set(done=4)
        progress.set("decode")
        self.assertEqual(progress.d["stage"], "decode")
        self.assertEqual(progress.d["done"], 0)
        self.assertEqual(progress.d["total"], 0)

    def assert_closed(self, stage=None):
        progress = self.progress()
        progress.set("read", total=10)
        progress.set(done=4)
        if stage is None:
            progress.close()
        else:
            progress.close(stage)
        state = self.read_state(progress)
        self.assertEqual(state["stage"], stage or "done")
        self.assertIs(state["final"], True)
        self.assertEqual(list(Path(self.directory).glob("*.tmp")), [])

    def test_close(self):
        self.assert_closed()

    def test_close_failed(self):
        self.assert_closed("failed")

    def test_background_writes_dirty_state(self):
        progress = self.progress()
        progress.set("read", total=10)
        previous = self.read_state(progress)
        progress.set(done=4)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            state = self.read_state(progress)
            if state["done"] == 4:
                break
            time.sleep(0.01)
        else:
            self.fail("Background thread did not write dirty state within 3 s")
        self.assertEqual(state["stage"], "read")
        self.assertEqual(state["total"], 10)
        self.assertGreaterEqual(state["updated"], previous["updated"])

    def test_concurrent_updates_always_publish_valid_json(self):
        progress = self.progress()
        progress.set("read", total=800)
        start = threading.Barrier(5)
        errors = queue.Queue()

        def worker(number):
            try:
                start.wait(timeout=1)
                for index in range(200):
                    # Supplying the stage forces a write on every call.
                    progress.set("read", done=number * 200 + index + 1)
            except Exception as error:
                errors.put(error)

        threads = [threading.Thread(target=worker, args=(n,), daemon=True)
                   for n in range(4)]
        for thread in threads:
            thread.start()
        reads = 0
        deadline = time.monotonic() + 2
        try:
            start.wait(timeout=1)
            while any(thread.is_alive() for thread in threads):
                self.assertLess(time.monotonic(), deadline,
                                "Concurrent writers did not finish promptly")
                state = self.read_state(progress)
                self.assertEqual(state["stage"], "read")
                self.assertIsInstance(state["done"], int)
                reads += 1
        finally:
            for thread in threads:
                thread.join(timeout=0.1)
        self.assertTrue(all(not thread.is_alive() for thread in threads))
        self.assertTrue(errors.empty(), list(errors.queue))
        self.assertGreater(reads, 1)
        self.assertEqual(self.read_state(progress)["total"], 800)


if __name__ == "__main__":
    unittest.main()
