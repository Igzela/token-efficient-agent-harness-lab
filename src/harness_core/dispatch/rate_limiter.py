"""Phase 6B-3: Per-tenant, per-API-key sliding window rate limiter."""

from __future__ import annotations

import bisect
import threading
import time
from dataclasses import dataclass


RATE_LIMITER_SCHEMA_VERSION = "rate_limiter.v1"

DEFAULT_WINDOW_SECONDS = 60.0
DEFAULT_MAX_BUCKETS = 10_000


@dataclass(frozen=True)
class RateLimitResult:
    allowed: bool
    remaining: int
    limit: int
    retry_after: float | None = None


class RateLimiter:
    def __init__(
        self,
        window_seconds: float = DEFAULT_WINDOW_SECONDS,
        max_buckets: int = DEFAULT_MAX_BUCKETS,
    ) -> None:
        self._window_seconds = window_seconds
        self._max_buckets = max_buckets
        self._buckets: dict[tuple[str, str], list[float]] = {}
        self._lock = threading.Lock()

    @property
    def window_seconds(self) -> float:
        return self._window_seconds

    def check(
        self,
        tenant_id: str,
        api_key_id: str,
        rate_limit: int | None,
        now: float | None = None,
    ) -> RateLimitResult:
        if rate_limit is None or rate_limit <= 0:
            return RateLimitResult(
                allowed=True,
                remaining=-1,
                limit=-1,
            )

        current_time = now if now is not None else time.time()
        window_start = current_time - self._window_seconds
        key = (tenant_id, api_key_id)

        with self._lock:
            # Evict oldest buckets if at capacity
            if key not in self._buckets and len(self._buckets) >= self._max_buckets:
                oldest_key = min(self._buckets, key=lambda k: self._buckets[k][0] if self._buckets[k] else float("inf"))
                del self._buckets[oldest_key]

            timestamps = self._buckets.setdefault(key, [])
            self._prune(timestamps, window_start)
            current_count = len(timestamps)

            if current_count >= rate_limit:
                oldest_in_window = timestamps[0]
                retry_after = oldest_in_window + self._window_seconds - current_time
                return RateLimitResult(
                    allowed=False,
                    remaining=0,
                    limit=rate_limit,
                    retry_after=max(retry_after, 0.0),
                )

            timestamps.append(current_time)
            return RateLimitResult(
                allowed=True,
                remaining=rate_limit - current_count - 1,
                limit=rate_limit,
            )

    def cleanup(self, now: float | None = None) -> int:
        current_time = now if now is not None else time.time()
        window_start = current_time - self._window_seconds
        removed = 0

        with self._lock:
            empty_keys = []
            for key, timestamps in self._buckets.items():
                before = len(timestamps)
                self._prune(timestamps, window_start)
                removed += before - len(timestamps)
                if not timestamps:
                    empty_keys.append(key)
            for key in empty_keys:
                del self._buckets[key]

        return removed

    def bucket_count(self) -> int:
        with self._lock:
            return len(self._buckets)

    def _prune(self, timestamps: list[float], window_start: float) -> None:
        cutoff = bisect.bisect_right(timestamps, window_start)
        if cutoff > 0:
            del timestamps[:cutoff]
