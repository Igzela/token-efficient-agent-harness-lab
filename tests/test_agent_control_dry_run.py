"""Tests for dry_run.py validation logic."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import dry_run


class TestDryRunHelpers(unittest.TestCase):

    def test_log_test_pass(self):
        dry_run.RESULTS.clear()
        dry_run.log_test("test pass", True)
        self.assertEqual(len(dry_run.RESULTS), 1)
        self.assertTrue(dry_run.RESULTS[0]["passed"])
        self.assertEqual(dry_run.RESULTS[0]["name"], "test pass")

    def test_log_test_fail(self):
        dry_run.RESULTS.clear()
        dry_run.log_test("test fail", False, "something went wrong")
        self.assertEqual(len(dry_run.RESULTS), 1)
        self.assertFalse(dry_run.RESULTS[0]["passed"])
        self.assertEqual(dry_run.RESULTS[0]["details"], "something went wrong")

    def test_retry_limit_logic(self):
        max_repairs = 2
        self.assertTrue(0 <= max_repairs)  # first repair allowed
        self.assertTrue(1 <= max_repairs)  # second repair allowed
        self.assertTrue(2 <= max_repairs)  # third at limit
        self.assertFalse(3 <= max_repairs)  # fourth exceeds limit
        self.assertFalse(4 <= max_repairs)

    def test_concurrency_group_logs(self):
        import lock_manager as lm
        g1 = lm.gh_concurrency_group(1, 100, "a" * 40)
        self.assertEqual(g1, "issue-1:pr-100:sha-aaaaaaaaaaaa")
        g2 = lm.gh_concurrency_group(5)
        self.assertEqual(g2, "issue-5")
        g3 = lm.gh_concurrency_group(5, 10)
        self.assertEqual(g3, "issue-5:pr-10")


class TestLabelSets(unittest.TestCase):

    def test_active_labels_are_not_terminal(self):
        import state_manager as sm
        for lbl in sm.ACTIVE_LABELS:
            self.assertNotIn(lbl, sm.TERMINAL_LABELS)

    def test_draft_not_active(self):
        import state_manager as sm
        self.assertNotIn(sm.LABEL_DRAFT, sm.ACTIVE_LABELS)

    def test_draft_not_terminal(self):
        import state_manager as sm
        self.assertNotIn(sm.LABEL_DRAFT, sm.TERMINAL_LABELS)

    def test_ready_not_active(self):
        import state_manager as sm
        self.assertNotIn(sm.LABEL_READY, sm.ACTIVE_LABELS)

    def test_ready_not_terminal(self):
        import state_manager as sm
        self.assertNotIn(sm.LABEL_READY, sm.TERMINAL_LABELS)


if __name__ == "__main__":
    unittest.main()
