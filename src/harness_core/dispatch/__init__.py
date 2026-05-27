"""Dispatch kernel: deterministic task analysis, model selection, budget reservation."""

from __future__ import annotations

from .budget_manager import BudgetManager
from .cost_of_pass import CostOfPassAccumulator
from .dispatch_decision import (
    BUDGET_RESERVATION_SCHEMA_VERSION,
    COMPLEXITY_WEIGHTS,
    CLEARANCE_VALUES,
    DECISION_STATUSES,
    DISPATCH_DECISION_SCHEMA_VERSION,
    EXECUTION_GATE_TYPES,
    EXECUTOR_TYPES,
    GATE_SEVERITIES,
    MODEL_TIERS,
    QUALITY_REQUIREMENTS,
    REQUEST_SOURCES,
    RISK_FLAGS,
    RISK_LEVELS,
    TASK_DOMAINS,
    TASK_INTENTS,
    BudgetReservation,
    DispatchDecision,
    Evidence,
    ExecutionGate,
    RejectedCandidate,
    ShadowRoute,
)
from .dispatch_engine import DispatchEngine
from .dispatch_ledger import DISPATCH_RECORD_SCHEMA_VERSION, DISPATCH_STATUSES, DispatchBundle, DispatchLedger, DispatchRecord
from .evaluation_stub import (
    EVAL_CHECK_NAMES,
    EVALUATION_RESULT_SCHEMA_VERSION,
    EvaluationCheck,
    EvaluationResult,
    EvaluationStub,
)
from .executor_adapter import (
    EXECUTION_RESULT_SCHEMA_VERSION,
    EXECUTION_STATUSES,
    ExecutionResult,
    ManualExecutor,
    MockExecutor,
    NoopExecutor,
)
from .manual_evaluator import ManualEvalCheck, ManualEvalResult, ManualEvaluator
from .manual_session import MANUAL_SESSION_SCHEMA_VERSION, MANUAL_SESSION_STATUSES, ManualExecutionSession, ManualSessionStore
from .manual_usage_bridge import ManualUsageBridge
from .model_selector import DispatchRoutingPolicy, ModelSelector
from .pasteback_parser import PASTEBACK_SUBMISSION_SCHEMA_VERSION, PastebackParser, PastebackSubmission
from .prompt_pack_gen import PROMPT_PACK_SCHEMA_VERSION, PromptPack, PromptPackGenerator
from .task_analyzer import TASK_ANALYSIS_SCHEMA_VERSION, RuleBasedTaskAnalyzer, TaskAnalysis

__all__ = [
    "BUDGET_RESERVATION_SCHEMA_VERSION",
    "BudgetManager",
    "BudgetReservation",
    "CLEARANCE_VALUES",
    "COMPLEXITY_WEIGHTS",
    "CostOfPassAccumulator",
    "DECISION_STATUSES",
    "DISPATCH_DECISION_SCHEMA_VERSION",
    "DISPATCH_RECORD_SCHEMA_VERSION",
    "DISPATCH_STATUSES",
    "DispatchBundle",
    "DispatchDecision",
    "DispatchEngine",
    "DispatchLedger",
    "DispatchRecord",
    "DispatchRoutingPolicy",
    "EVAL_CHECK_NAMES",
    "EVALUATION_RESULT_SCHEMA_VERSION",
    "EXECUTION_GATE_TYPES",
    "EXECUTION_RESULT_SCHEMA_VERSION",
    "EXECUTION_STATUSES",
    "EXECUTOR_TYPES",
    "Evidence",
    "EvaluationCheck",
    "EvaluationResult",
    "EvaluationStub",
    "ExecutionGate",
    "ExecutionResult",
    "GATE_SEVERITIES",
    "MANUAL_EVAL_RESULT_SCHEMA_VERSION",
    "MANUAL_SESSION_SCHEMA_VERSION",
    "MANUAL_SESSION_STATUSES",
    "MANUAL_EXECUTION_SESSION_SCHEMA_VERSION",
    "PASTEBACK_SUBMISSION_SCHEMA_VERSION",
    "PROMPT_PACK_SCHEMA_VERSION",
    "ManualEvalCheck",
    "ManualEvalResult",
    "ManualEvaluator",
    "ManualExecutionSession",
    "ManualExecutor",
    "ManualSessionStore",
    "ManualUsageBridge",
    "MODEL_TIERS",
    "MockExecutor",
    "ModelSelector",
    "NoopExecutor",
    "PastebackParser",
    "PastebackSubmission",
    "PromptPack",
    "PromptPackGenerator",
    "QUALITY_REQUIREMENTS",
    "REQUEST_SOURCES",
    "RISK_FLAGS",
    "RISK_LEVELS",
    "RejectedCandidate",
    "RuleBasedTaskAnalyzer",
    "ShadowRoute",
    "TASK_ANALYSIS_SCHEMA_VERSION",
    "TASK_DOMAINS",
    "TASK_INTENTS",
    "TaskAnalysis",
]
