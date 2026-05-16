"""Health aggregation for Stage 4 runtime components."""

from __future__ import annotations

from dataclasses import dataclass

from .supervisor import ComponentHealth, SupervisorReport


@dataclass(frozen=True)
class HealthReport:
    checked_at: int
    overall_status: str  # "healthy" | "degraded" | "failed"
    components: tuple[ComponentHealth, ...]
    warnings: tuple[str, ...] = ()

    @property
    def healthy(self) -> bool:
        return self.overall_status == "healthy"


class HealthMonitor:
    """Aggregate supplied component health without probing real processes."""

    def aggregate(
        self,
        components: tuple[ComponentHealth, ...],
        *,
        checked_at: int,
        warnings: tuple[str, ...] = (),
    ) -> HealthReport:
        status = "healthy"
        if any(component.status == "failed" for component in components):
            status = "failed"
        elif any(component.status == "degraded" for component in components):
            status = "degraded"
        return HealthReport(
            checked_at=checked_at,
            overall_status=status,
            components=tuple(sorted(components, key=lambda c: c.component_id)),
            warnings=tuple(warnings),
        )

    def from_supervisor_report(
        self,
        report: SupervisorReport,
        *,
        extra_components: tuple[ComponentHealth, ...] = (),
    ) -> HealthReport:
        warnings = tuple(
            component.message
            for component in report.component_health
            if component.status != "healthy"
        )
        return self.aggregate(
            report.component_health + tuple(extra_components),
            checked_at=report.checked_at,
            warnings=warnings,
        )
