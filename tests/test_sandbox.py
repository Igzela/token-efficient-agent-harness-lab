import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.sandbox import ConflictReport, FileClaim, Sandbox, SandboxManager


class SandboxCreateTests(unittest.TestCase):
    def test_create_empty_sandbox(self):
        mgr = SandboxManager()
        sbx = mgr.create_sandbox("task_1", ())
        self.assertEqual("created", sbx.status)
        self.assertEqual("task_1", sbx.task_id)
        self.assertEqual((), sbx.claimed_files)

    def test_create_with_files(self):
        mgr = SandboxManager()
        sbx = mgr.create_sandbox("task_1", ("a.py", "b.py"))
        self.assertEqual("active", sbx.status)
        self.assertEqual(("a.py", "b.py"), sbx.claimed_files)

    def test_sandbox_id_is_deterministic(self):
        mgr = SandboxManager()
        sbx1 = mgr.create_sandbox("task_1", ())
        sbx2 = mgr.create_sandbox("task_1", ())
        self.assertNotEqual(sbx1.sandbox_id, sbx2.sandbox_id)


class SandboxClaimTests(unittest.TestCase):
    def test_claim_files_ok(self):
        mgr = SandboxManager()
        sbx = mgr.create_sandbox("task_1", ())
        report = mgr.claim_files(sbx.sandbox_id, ("a.py",))
        self.assertFalse(report.has_conflict)
        self.assertEqual("active", mgr.get_sandbox(sbx.sandbox_id).status)
        self.assertEqual(sbx.sandbox_id, mgr.is_file_claimed("a.py"))

    def test_claim_conflict_different_sandbox(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        s2 = mgr.create_sandbox("task_2", ())
        report = mgr.claim_files(s2.sandbox_id, ("a.py",))
        self.assertTrue(report.has_conflict)
        self.assertEqual(s1.sandbox_id, report.conflicting_sandbox_id)
        self.assertEqual("a.py", report.conflicting_file)

    def test_claim_same_sandbox_no_conflict(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        report = mgr.claim_files(s1.sandbox_id, ("a.py",))
        self.assertFalse(report.has_conflict)

    def test_claim_unknown_sandbox(self):
        mgr = SandboxManager()
        report = mgr.claim_files("nonexistent", ("a.py",))
        self.assertTrue(report.has_conflict)
        self.assertIn("unknown sandbox", report.message)

    def test_claim_on_released_sandbox(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ())
        mgr.release_sandbox(s1.sandbox_id)
        report = mgr.claim_files(s1.sandbox_id, ("a.py",))
        self.assertTrue(report.has_conflict)
        self.assertIn("released", report.message)


class SandboxReleaseTests(unittest.TestCase):
    def test_release_frees_files(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        mgr.release_sandbox(s1.sandbox_id)
        self.assertIsNone(mgr.is_file_claimed("a.py"))
        self.assertEqual("released", mgr.get_sandbox(s1.sandbox_id).status)

    def test_release_unknown_sandbox_raises(self):
        mgr = SandboxManager()
        with self.assertRaises(ValueError):
            mgr.release_sandbox("nonexistent")

    def test_release_already_released_is_idempotent(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        mgr.release_sandbox(s1.sandbox_id)
        sbx = mgr.release_sandbox(s1.sandbox_id)
        self.assertEqual("released", sbx.status)

    def test_released_sandbox_allows_new_owner(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        mgr.release_sandbox(s1.sandbox_id)
        s2 = mgr.create_sandbox("task_2", ("a.py",))
        self.assertEqual(s2.sandbox_id, mgr.is_file_claimed("a.py"))


class SandboxListTests(unittest.TestCase):
    def test_list_active(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ())
        s2 = mgr.create_sandbox("task_2", ())
        mgr.release_sandbox(s1.sandbox_id)
        active = mgr.list_active()
        self.assertEqual(1, len(active))
        self.assertEqual(s2.sandbox_id, active[0].sandbox_id)

    def test_list_all(self):
        mgr = SandboxManager()
        mgr.create_sandbox("task_1", ())
        mgr.create_sandbox("task_2", ())
        self.assertEqual(2, len(mgr.list_all()))


class SandboxGetClaimsTests(unittest.TestCase):
    def test_get_claims(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py", "b.py"))
        claims = mgr.get_claims(s1.sandbox_id)
        self.assertEqual(2, len(claims))
        paths = {c.file_path for c in claims}
        self.assertEqual({"a.py", "b.py"}, paths)

    def test_claims_mark_released(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        mgr.release_sandbox(s1.sandbox_id)
        claims = mgr.get_claims(s1.sandbox_id)
        self.assertTrue(all(c.released for c in claims))


class SandboxIsFileClaimedTests(unittest.TestCase):
    def test_unclaimed_file(self):
        mgr = SandboxManager()
        self.assertIsNone(mgr.is_file_claimed("a.py"))

    def test_claimed_file_returns_sandbox_id(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        self.assertEqual(s1.sandbox_id, mgr.is_file_claimed("a.py"))

    def test_released_file_unclaimed(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        mgr.release_sandbox(s1.sandbox_id)
        self.assertIsNone(mgr.is_file_claimed("a.py"))


class SandboxMultipleFilesConflictTests(unittest.TestCase):
    def test_partial_conflict(self):
        mgr = SandboxManager()
        s1 = mgr.create_sandbox("task_1", ("a.py",))
        s2 = mgr.create_sandbox("task_2", ())
        report = mgr.claim_files(s2.sandbox_id, ("a.py", "b.py"))
        self.assertTrue(report.has_conflict)
        self.assertEqual("a.py", report.conflicting_file)
        self.assertIsNone(mgr.is_file_claimed("b.py"))


if __name__ == "__main__":
    unittest.main()
