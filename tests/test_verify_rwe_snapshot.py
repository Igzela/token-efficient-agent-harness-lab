import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts import verify_rwe_snapshot


class VerifyRweSnapshotTests(unittest.TestCase):
    def test_canonical_manifest_digest_is_stable_and_field_bound(self) -> None:
        manifest = {"schema_version": "test", "value": 1}
        first = verify_rwe_snapshot.canonical_manifest_digest(manifest)
        manifest["value"] = 2
        self.assertNotEqual(first, verify_rwe_snapshot.canonical_manifest_digest(manifest))

    def test_missing_required_sections_fail_closed(self) -> None:
        manifest = {"schema_version": "test"}
        manifest["manifest_sha256"] = verify_rwe_snapshot.canonical_manifest_digest(manifest)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "manifest.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            failures = verify_rwe_snapshot.verify(path, root, root)
        self.assertTrue(any("manifest section is missing or malformed" in item for item in failures))

    def test_verify_file_rejects_wrong_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "input.txt"
            path.write_text("bound", encoding="utf-8")
            failures: list[str] = []
            verify_rwe_snapshot.verify_file(root, "input.txt", hashlib.sha256(b"other").hexdigest(), failures)
        self.assertEqual(failures, ["snapshot hash mismatch: input.txt"])

    def test_git_overlay_paths_include_staged_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for args in (
                ("init", "-b", "main"),
                ("config", "user.name", "Test"),
                ("config", "user.email", "test@example.invalid"),
            ):
                subprocess.run(("git", *args), cwd=root, check=True, capture_output=True)
            (root / "base.txt").write_text("base\n", encoding="utf-8")
            subprocess.run(("git", "add", "base.txt"), cwd=root, check=True)
            subprocess.run(("git", "commit", "-m", "base"), cwd=root, check=True, capture_output=True)
            (root / "recipe.txt").write_text("recipe\n", encoding="utf-8")
            (root / "staged.txt").write_text("staged\n", encoding="utf-8")
            subprocess.run(("git", "add", "staged.txt"), cwd=root, check=True)

            self.assertEqual(
                verify_rwe_snapshot.git_overlay_paths(root),
                ["recipe.txt", "staged.txt"],
            )


if __name__ == "__main__":
    unittest.main()
