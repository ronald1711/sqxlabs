import concurrent.futures
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from retain_engine import retain


class RetainEngineTests(unittest.TestCase):
    def test_concurrent_retention_survives_original_replacement(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "engine"
            original = b"original engine\x00" * 100000
            source.write_bytes(original)
            identity = hashlib.sha256(original).hexdigest()
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                paths = list(pool.map(lambda _: retain(source, root / "store", identity), range(8)))
            self.assertEqual(len(set(paths)), 1)
            source.write_bytes(b"new version")
            self.assertEqual(paths[0].read_bytes(), original)
            self.assertEqual(paths[0].stat().st_mode & 0o111, 0)
            self.assertEqual(list((root / "store").glob(".pending-*")), [])

    def test_wrong_identity_and_corruption_fail_without_replacement(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "engine"
            source.write_bytes(b"engine")
            with self.assertRaisesRegex(ValueError, "does not match"):
                retain(source, root / "store", "0" * 64)
            self.assertEqual(list((root / "store").iterdir()), [])
            saved = retain(source, root / "store")
            saved.write_bytes(b"corrupt")
            with self.assertRaisesRegex(ValueError, "integrity"):
                retain(source, root / "store")
            self.assertEqual(saved.read_bytes(), b"corrupt")

    def test_symlink_is_not_accepted_as_retained_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "engine"
            source.write_bytes(b"engine")
            saved = retain(source, root / "store")
            saved.unlink()
            saved.symlink_to(source)
            with self.assertRaisesRegex(ValueError, "regular file"):
                retain(source, root / "store")
