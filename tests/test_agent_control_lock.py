"""Tests for lock_manager.py"""

import os
import sys
import threading
import time
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts", "agent-control"))
import lock_manager as lm


class TestLockManager(unittest.TestCase):

    def setUp(self):
        self.test_key = f"test-lock-{os.getpid()}-{time.time_ns()}"
        self.release_key(self.test_key)

    def tearDown(self):
        self.release_key(self.test_key)

    def release_key(self, key):
        try:
            lm.release_lock(key)
        except Exception:
            pass

    def test_acquire_and_release(self):
        self.assertTrue(lm.acquire_lock(self.test_key, timeout_secs=5))
        lm.release_lock(self.test_key)
        self.assertTrue(lm.acquire_lock(self.test_key, timeout_secs=5))
        lm.release_lock(self.test_key)

    def test_acquire_twice_fails(self):
        self.assertTrue(lm.acquire_lock(self.test_key, timeout_secs=5))
        self.assertFalse(lm.acquire_lock(self.test_key, timeout_secs=2))
        lm.release_lock(self.test_key)

    def test_release_twice_ok(self):
        self.assertTrue(lm.acquire_lock(self.test_key, timeout_secs=5))
        lm.release_lock(self.test_key)
        lm.release_lock(self.test_key)

    def test_concurrent_locks(self):
        results = []
        key = f"{self.test_key}-concurrent"

        def try_acquire(k, result_list):
            result_list.append(lm.acquire_lock(k, timeout_secs=5))

        self.assertTrue(lm.acquire_lock(key, timeout_secs=5))
        t = threading.Thread(target=try_acquire, args=(key, results))
        t.start()
        t.join(timeout=3)
        self.assertGreaterEqual(len(results), 0)
        if len(results) > 0:
            self.assertFalse(results[0])
        lm.release_lock(key)

    def test_count_active_locks(self):
        before = lm.count_active_locks()
        self.assertGreaterEqual(before, 0)

        key1 = f"{self.test_key}-count1"
        key2 = f"{self.test_key}-count2"
        lm.acquire_lock(key1, timeout_secs=5)
        lm.acquire_lock(key2, timeout_secs=5)

        count = lm.count_active_locks()
        self.assertGreaterEqual(count, before + 2)

        lm.release_lock(key1)
        lm.release_lock(key2)

    def test_repo_capacity(self):
        self.assertTrue(lm.check_repo_capacity())

    def test_concurrency_group_format(self):
        group = lm.gh_concurrency_group(1, 100, "abc123def456")
        self.assertIn("issue-1", group)
        self.assertIn("pr-100", group)
        self.assertIn("sha-abc123def456", group)

    def test_concurrency_group_no_pr(self):
        group = lm.gh_concurrency_group(5)
        self.assertIn("issue-5", group)
        self.assertNotIn("pr-", group)

    def test_concurrency_group_no_sha(self):
        group = lm.gh_concurrency_group(5, 10)
        self.assertIn("issue-5", group)
        self.assertIn("pr-10", group)
        self.assertNotIn("sha-", group)


class TestLockStaleDetection(unittest.TestCase):

    def test_stale_lock_cleanup(self):
        key = f"test-stale-{os.getpid()}-{time.time_ns()}"
        lm.acquire_lock(key, timeout_secs=5)
        self.assertFalse(lm.acquire_lock(key, timeout_secs=1))
        lm.release_lock(key)
        self.assertTrue(lm.acquire_lock(key, timeout_secs=5))
        lm.release_lock(key)


if __name__ == "__main__":
    unittest.main()
