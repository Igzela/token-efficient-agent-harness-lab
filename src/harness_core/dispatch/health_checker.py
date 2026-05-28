"""Phase 6A: HealthChecker — storage and service health probes."""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any


HEALTH_CHECKER_SCHEMA_VERSION = "health_checker.v1"


@dataclass(frozen=True)
class HealthCheck:
    name: str
    status: str
    message: str = ""
    latency_ms: float = 0.0


@dataclass(frozen=True)
class HealthReport:
    status: str
    checks: list[HealthCheck]
    timestamp: float = field(default_factory=time.time)


class HealthChecker:
    """Probes storage, events, and plan stores for health/readiness."""

    def __init__(self, store: Any = None) -> None:
        self._store = store

    def check_storage(self) -> HealthCheck:
        if self._store is None:
            return HealthCheck(name="storage", status="unhealthy", message="no store configured")
        start = time.monotonic()
        try:
            stats = self._store.stats()
            latency = (time.monotonic() - start) * 1000
            return HealthCheck(name="storage", status="healthy",
                               message=f"plans={stats['plans']}, repos={stats['repos']}, events={stats['events']}",
                               latency_ms=latency)
        except Exception as e:
            latency = (time.monotonic() - start) * 1000
            return HealthCheck(name="storage", status="unhealthy", message=str(e), latency_ms=latency)

    def check_events(self) -> HealthCheck:
        if self._store is None:
            return HealthCheck(name="events", status="unhealthy", message="no store configured")
        start = time.monotonic()
        try:
            events = self._store.get_events(limit=1)
            latency = (time.monotonic() - start) * 1000
            return HealthCheck(name="events", status="healthy",
                               message=f"accessible, latest_count={len(events)}",
                               latency_ms=latency)
        except Exception as e:
            latency = (time.monotonic() - start) * 1000
            return HealthCheck(name="events", status="unhealthy", message=str(e), latency_ms=latency)

    def check_plans(self) -> HealthCheck:
        if self._store is None:
            return HealthCheck(name="plans", status="unhealthy", message="no store configured")
        start = time.monotonic()
        try:
            plans = self._store.list_plans()
            latency = (time.monotonic() - start) * 1000
            return HealthCheck(name="plans", status="healthy",
                               message=f"accessible, count={len(plans)}",
                               latency_ms=latency)
        except Exception as e:
            latency = (time.monotonic() - start) * 1000
            return HealthCheck(name="plans", status="unhealthy", message=str(e), latency_ms=latency)

    def health(self) -> HealthReport:
        checks = [
            self.check_storage(),
            self.check_events(),
            self.check_plans(),
        ]
        statuses = [c.status for c in checks]
        if all(s == "healthy" for s in statuses):
            overall = "healthy"
        elif any(s == "unhealthy" for s in statuses):
            overall = "unhealthy"
        else:
            overall = "degraded"
        return HealthReport(status=overall, checks=checks)

    def readiness(self) -> HealthReport:
        checks = [
            self.check_storage(),
            self.check_events(),
            self.check_plans(),
        ]
        ready = all(c.status == "healthy" for c in checks)
        return HealthReport(
            status="ready" if ready else "not_ready",
            checks=checks,
        )

    def health_dict(self) -> dict[str, Any]:
        report = self.health()
        return {
            "status": report.status,
            "checks": {c.name: {"status": c.status, "message": c.message, "latency_ms": c.latency_ms}
                       for c in report.checks},
            "timestamp": report.timestamp,
        }

    def readiness_dict(self) -> dict[str, Any]:
        report = self.readiness()
        return {
            "ready": report.status == "ready",
            "status": report.status,
            "checks": {c.name: {"status": c.status, "message": c.message}
                       for c in report.checks},
        }
