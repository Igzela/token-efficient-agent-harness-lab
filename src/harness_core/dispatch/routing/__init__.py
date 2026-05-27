"""Phase 4: Adaptive routing subpackage."""

from __future__ import annotations

from .auto_policies import AutoDowngradePolicy, AutoUpgradePolicy
from .cost_of_pass_router import CostOfPassRouter
from .dynamic_tier_selector import DynamicTierSelector
from .feedback_integrator import FeedbackIntegrator
from .history_store import RoutingHistoryStore
from .promotion_gate import PromotionGate, PromotionVerdict, RoutingObservationStore
from .schemas import (
    DOWNGRADE_REASONS,
    EXPERIMENT_CONCLUSIONS,
    EXPERIMENT_STATUSES,
    PROMOTION_GATE_DEFAULTS,
    PROMOTION_VERDICTS,
    ROUTING_ARM_SCHEMA_VERSION,
    ROUTING_EXPERIMENT_SCHEMA_VERSION,
    ROUTING_MODES,
    ROUTING_OBSERVATION_SCHEMA_VERSION,
    RoutingArm,
    RoutingExperiment,
    RoutingObservation,
)

__all__ = [
    "AutoDowngradePolicy",
    "AutoUpgradePolicy",
    "CostOfPassRouter",
    "DOWNGRADE_REASONS",
    "DynamicTierSelector",
    "EXPERIMENT_CONCLUSIONS",
    "EXPERIMENT_STATUSES",
    "FeedbackIntegrator",
    "PROMOTION_GATE_DEFAULTS",
    "PROMOTION_VERDICTS",
    "PromotionGate",
    "PromotionVerdict",
    "ROUTING_ARM_SCHEMA_VERSION",
    "ROUTING_EXPERIMENT_SCHEMA_VERSION",
    "ROUTING_MODES",
    "ROUTING_OBSERVATION_SCHEMA_VERSION",
    "RoutingArm",
    "RoutingExperiment",
    "RoutingHistoryStore",
    "RoutingObservation",
    "RoutingObservationStore",
]
