"""Tests for dispatch/rate_limiter.py — sliding window rate limiter."""

import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.rate_limiter import (
    DEFAULT_WINDOW_SECONDS,
    RATE_LIMITER_SCHEMA_VERSION,
    RateLimitResult,
    RateLimiter,
)


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version(self):
        self.assertEqual(RATE_LIMITER_SCHEMA_VERSION, "rate_limiter.v1")


class DefaultWindowTests(unittest.TestCase):
    def test_default_window_is_60_seconds(self):
        self.assertEqual(DEFAULT_WINDOW_SECONDS, 60.0)

    def test_custom_window(self):
        limiter = RateLimiter(window_seconds=30.0)
        self.assertEqual(limiter.window_seconds, 30.0)


class UnlimitedTests(unittest.TestCase):
    def test_none_rate_limit_allows_all(self):
        limiter = RateLimiter()
        result = limiter.check("t", "k", rate_limit=None)
        self.assertTrue(result.allowed)
        self.assertEqual(result.remaining, -1)
        self.assertEqual(result.limit, -1)

    def test_zero_rate_limit_allows_all(self):
        limiter = RateLimiter()
        result = limiter.check("t", "k", rate_limit=0)
        self.assertTrue(result.allowed)
        self.assertEqual(result.remaining, -1)

    def test_negative_rate_limit_allows_all(self):
        limiter = RateLimiter()
        result = limiter.check("t", "k", rate_limit=-5)
        self.assertTrue(result.allowed)
        self.assertEqual(result.remaining, -1)


class SlidingWindowTests(unittest.TestCase):
    def test_within_limit(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        r1 = limiter.check("t", "k", rate_limit=3, now=t)
        self.assertTrue(r1.allowed)
        self.assertEqual(r1.remaining, 2)
        self.assertIsNone(r1.retry_after)

    def test_at_limit(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=2, now=t)
        limiter.check("t", "k", rate_limit=2, now=t + 0.1)
        r = limiter.check("t", "k", rate_limit=2, now=t + 0.2)
        self.assertFalse(r.allowed)
        self.assertEqual(r.remaining, 0)
        self.assertEqual(r.limit, 2)

    def test_over_limit_returns_retry_after(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=1, now=t)
        r = limiter.check("t", "k", rate_limit=1, now=t + 5.0)
        self.assertFalse(r.allowed)
        self.assertAlmostEqual(r.retry_after, 55.0, places=1)

    def test_single_request_allowed(self):
        limiter = RateLimiter()
        r = limiter.check("t", "k", rate_limit=1)
        self.assertTrue(r.allowed)
        self.assertEqual(r.remaining, 0)


class TenantIsolationTests(unittest.TestCase):
    def test_different_tenants_independent(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        limiter.check("t1", "k", rate_limit=1, now=t)
        r = limiter.check("t2", "k", rate_limit=1, now=t)
        self.assertTrue(r.allowed)


class KeyIsolationTests(unittest.TestCase):
    def test_different_keys_independent(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        limiter.check("t", "k1", rate_limit=1, now=t)
        r = limiter.check("t", "k2", rate_limit=1, now=t)
        self.assertTrue(r.allowed)


class WindowExpiryTests(unittest.TestCase):
    def test_entries_expire_after_window(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=1, now=t)
        r = limiter.check("t", "k", rate_limit=1, now=t + 10.0)
        self.assertTrue(r.allowed)

    def test_entries_not_yet_expired(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=1, now=t)
        r = limiter.check("t", "k", rate_limit=1, now=t + 9.9)
        self.assertFalse(r.allowed)

    def test_partial_window_expiry(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=2, now=t)
        limiter.check("t", "k", rate_limit=2, now=t + 5.0)
        r1 = limiter.check("t", "k", rate_limit=2, now=t + 8.0)
        self.assertFalse(r1.allowed)
        r2 = limiter.check("t", "k", rate_limit=2, now=t + 10.0)
        self.assertTrue(r2.allowed)


class CleanupTests(unittest.TestCase):
    def test_cleanup_removes_expired_entries(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=5, now=t)
        limiter.check("t", "k", rate_limit=5, now=t + 1.0)
        self.assertEqual(limiter.bucket_count(), 1)
        removed = limiter.cleanup(now=t + 11.0)
        self.assertEqual(removed, 2)
        self.assertEqual(limiter.bucket_count(), 0)

    def test_cleanup_keeps_valid_entries(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=5, now=t)
        removed = limiter.cleanup(now=t + 5.0)
        self.assertEqual(removed, 0)
        self.assertEqual(limiter.bucket_count(), 1)

    def test_cleanup_returns_total_removed(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        limiter.check("t", "k1", rate_limit=3, now=t)
        limiter.check("t", "k2", rate_limit=3, now=t)
        limiter.check("t", "k1", rate_limit=3, now=t + 1.0)
        removed = limiter.cleanup(now=t + 11.0)
        self.assertEqual(removed, 3)


class ThreadSafetyTests(unittest.TestCase):
    def test_concurrent_checks_do_not_crash(self):
        limiter = RateLimiter(window_seconds=60.0)
        errors = []

        def worker():
            try:
                for i in range(100):
                    limiter.check("t", "k", rate_limit=50)
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=worker) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(errors, [])

    def test_concurrent_checks_respect_limit(self):
        limiter = RateLimiter(window_seconds=60.0)
        results = []
        lock = threading.Lock()

        def worker():
            for _ in range(20):
                r = limiter.check("t", "k", rate_limit=50)
                with lock:
                    results.append(r)

        threads = [threading.Thread(target=worker) for _ in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        denied = [r for r in results if not r.allowed]
        allowed = [r for r in results if r.allowed]
        self.assertEqual(len(allowed), 50)
        self.assertEqual(len(denied), 50)

    def test_concurrent_cleanup_safe(self):
        limiter = RateLimiter(window_seconds=10.0)
        t = 1000.0
        for i in range(5):
            limiter.check("t", f"k{i}", rate_limit=5, now=t)

        errors = []

        def checker():
            try:
                for _ in range(50):
                    limiter.check("t", "k0", rate_limit=5)
            except Exception as e:
                errors.append(e)

        def cleaner():
            try:
                for _ in range(50):
                    limiter.cleanup()
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=checker) for _ in range(3)]
        threads += [threading.Thread(target=cleaner) for _ in range(2)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(errors, [])


class RateLimitResultTests(unittest.TestCase):
    def test_frozen_dataclass(self):
        r = RateLimitResult(allowed=True, remaining=5, limit=10)
        self.assertTrue(r.allowed)
        self.assertEqual(r.remaining, 5)
        self.assertEqual(r.limit, 10)
        self.assertIsNone(r.retry_after)

    def test_retry_after_field(self):
        r = RateLimitResult(allowed=False, remaining=0, limit=1, retry_after=30.0)
        self.assertEqual(r.retry_after, 30.0)


class RemainingCountTests(unittest.TestCase):
    def test_remaining_decrements(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        r1 = limiter.check("t", "k", rate_limit=5, now=t)
        self.assertEqual(r1.remaining, 4)
        r2 = limiter.check("t", "k", rate_limit=5, now=t + 0.1)
        self.assertEqual(r2.remaining, 3)
        r3 = limiter.check("t", "k", rate_limit=5, now=t + 0.2)
        self.assertEqual(r3.remaining, 2)


class RetryAfterTests(unittest.TestCase):
    def test_retry_after_at_exact_limit(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=1, now=t)
        r = limiter.check("t", "k", rate_limit=1, now=t)
        self.assertFalse(r.allowed)
        self.assertAlmostEqual(r.retry_after, 60.0, places=1)

    def test_retry_after_decreases_over_time(self):
        limiter = RateLimiter(window_seconds=60.0)
        t = 1000.0
        limiter.check("t", "k", rate_limit=1, now=t)
        r1 = limiter.check("t", "k", rate_limit=1, now=t + 10.0)
        r2 = limiter.check("t", "k", rate_limit=1, now=t + 30.0)
        self.assertGreater(r1.retry_after, r2.retry_after)


if __name__ == "__main__":
    unittest.main()
