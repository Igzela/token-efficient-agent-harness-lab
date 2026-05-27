"""Rule-based task analyzer for the dispatch kernel.

Pure rules, deterministic, testable, no model calls.
"""

from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

from .dispatch_decision import (
    COMPLEXITY_WEIGHTS,
    QUALITY_REQUIREMENTS,
    RISK_FLAGS,
    RISK_LEVELS,
    TASK_DOMAINS,
    TASK_INTENTS,
    Evidence,
)

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

TASK_ANALYSIS_SCHEMA_VERSION = "task_analysis.v1"

# ---------------------------------------------------------------------------
# Negation phrases (adapted from resource_planner.py)
# ---------------------------------------------------------------------------

_NEGATED_RISK_PHRASES: tuple[str, ...] = (
    "no target repo writes",
    "no target repository writes",
    "do not write target repo",
    "do not write target repository",
    "target repo remains read-only",
    "target repository remains read-only",
    "without target repo writes",
    "without target repository writes",
    "no source changes",
    "does not modify target repo",
    "does not modify target repository",
    "no target repository mutation",
    "no target repo mutation",
    "without any provider calls",
    "without provider calls",
    "without model calls",
    "without any model calls",
    "no provider calls",
    "no model calls",
    "do not call providers",
    "do not call any providers",
    "no api key",
    "no credentials",
    "without any sandbox execution",
    "without sandbox execution",
    "without executing commands",
    "no sandbox execution",
    "no sandbox",
    "do not run sandbox",
    "no container",
    "no worker",
    "no autonomous workers",
    "read-only validation",
    "audit only",
    "review only",
)

# ---------------------------------------------------------------------------
# Keyword maps for classification
# ---------------------------------------------------------------------------

_DOMAIN_KEYWORDS: dict[str, tuple[str, ...]] = {
    "code": ("function", "class", "module", "implement", "refactor", "debug", "bug", "fix", "endpoint", "method", "variable", "auth.py", "test_auth"),
    "docs": ("document", "documentation", "readme", "docs", "guide", "tutorial", "write docs", "update docs"),
    "config": ("config", "configuration", "settings", "yaml", "toml", "env", "environment", ".env", "ci/cd", "pipeline config"),
    "infra": ("infrastructure", "deploy", "deployment", "docker", "kubernetes", "k8s", "terraform", "aws", "cloud", "server", "container"),
    "math": ("calculate", "compute", "formula", "equation", "algorithm", "batch size", "mathematical", "optimal batch"),
    "architecture": ("architecture", "system design", "microservice", "architectural", "high-level design", "component design"),
    "repo_ops": ("commit", "push", "merge", "branch", "pull request", "pr", "git", "repository", "repo", "clone", "fork"),
    "governance": ("governance", "compliance", "audit", "policy", "security audit", "vulnerability", "security review", "security"),
}

_INTENT_KEYWORDS: dict[str, tuple[str, ...]] = {
    "generate": ("generate", "create", "build", "implement", "add", "new", "scaffold", "produce"),
    "review": ("review", "look at", "analyze for issues", "examine"),
    "debug": ("debug", "fix", "bug", "error", "failing", "broken", "troubleshoot", "diagnose"),
    "summarize": ("summarize", "summary", "overview", "tldr", "short version", "condensed"),
    "audit": ("audit", "compliance", "vulnerability", "scan", "penetration", "security audit", "security review"),
    "plan": ("plan", "strategy", "roadmap", "approach", "design plan", "implementation plan"),
    "refactor": ("refactor", "restructure", "reorganize", "clean up", "improve code", "code quality"),
    "compare": ("compare", "contrast", "versus", "vs", "difference", "trade-off"),
    "explain": ("explain", "describe", "how does", "what is", "clarify", "elaborate"),
    "classify": ("classify", "categorize", "sort", "group", "label", "tag"),
}

_RISK_KEYWORDS: dict[str, tuple[str, ...]] = {
    "target_write": ("write", "modify", "edit", "commit", "push", "merge", "delete", "remove", "create file", "fix and commit"),
    "provider_call": ("call openai", "call anthropic", "openai api", "anthropic api", "provider call", "model call", "llm call", "gpt api", "claude api"),
    "sandbox_execution": ("sandbox", "container", "docker run", "shell command"),
    "deployment": ("deploy", "release", "publish", "production", "staging", "ship"),
    "secret_handling": ("secret", "api key", "credential", "password", "rotate key", "rotate the api"),
    "destructive_operation": ("delete", "drop table", "destroy", "wipe", "purge", "truncate", "rm -rf"),
    "long_context": ("500-file", "large codebase", "entire repo", "all files", "full codebase", "massive", "huge"),
    "high_uncertainty": ("unclear", "ambiguous", "not sure", "maybe", "might", "possibly", "make it better"),
}

# Budget estimation by domain × intent complexity
_BUDGET_BASE: dict[str, int] = {
    "code": 3000, "docs": 2000, "config": 1500, "infra": 2500,
    "math": 2000, "architecture": 3500, "repo_ops": 1500, "governance": 2000, "other": 2000,
}
_INTENT_MULTIPLIER: dict[str, float] = {
    "generate": 1.5, "review": 1.0, "debug": 1.3, "summarize": 0.7, "audit": 1.2,
    "plan": 1.4, "refactor": 1.3, "compare": 1.1, "explain": 0.9, "classify": 0.8,
}


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class TaskAnalysis:
    analysis_id: str
    raw_request_snapshot: str
    request_source: str  # from REQUEST_SOURCES
    primary_task_type: str
    task_domain: str  # from TASK_DOMAINS
    task_intent: str  # from TASK_INTENTS
    risk_flags: tuple[str, ...]
    complexity_score: float
    cognitive_complexity: float
    context_complexity: float
    execution_risk: float
    ambiguity_score: float
    required_capabilities: tuple[str, ...]
    context_budget_estimate: int
    execution_budget_estimate: int
    quality_requirement: str  # from QUALITY_REQUIREMENTS
    risk_level: str  # from RISK_LEVELS
    confidence: float
    confidence_label: str  # "low" | "medium" | "high"
    uncertainty_reason: tuple[str, ...]
    safe_default: str
    escalation_trigger: str | None
    positive_evidence: tuple[Evidence, ...]
    negative_evidence: tuple[Evidence, ...]
    features_detected: dict[str, Any]
    analysis_method: str  # literal "rule_only"
    created_at: str
    schema_version: str = TASK_ANALYSIS_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "analysis_id": self.analysis_id,
            "raw_request_snapshot": self.raw_request_snapshot,
            "request_source": self.request_source,
            "primary_task_type": self.primary_task_type,
            "task_domain": self.task_domain,
            "task_intent": self.task_intent,
            "risk_flags": list(self.risk_flags),
            "complexity_score": self.complexity_score,
            "cognitive_complexity": self.cognitive_complexity,
            "context_complexity": self.context_complexity,
            "execution_risk": self.execution_risk,
            "ambiguity_score": self.ambiguity_score,
            "required_capabilities": list(self.required_capabilities),
            "context_budget_estimate": self.context_budget_estimate,
            "execution_budget_estimate": self.execution_budget_estimate,
            "quality_requirement": self.quality_requirement,
            "risk_level": self.risk_level,
            "confidence": self.confidence,
            "confidence_label": self.confidence_label,
            "uncertainty_reason": list(self.uncertainty_reason),
            "safe_default": self.safe_default,
            "escalation_trigger": self.escalation_trigger,
            "positive_evidence": [e.to_dict() for e in self.positive_evidence],
            "negative_evidence": [e.to_dict() for e in self.negative_evidence],
            "features_detected": self.features_detected,
            "analysis_method": self.analysis_method,
            "created_at": self.created_at,
        }


# ---------------------------------------------------------------------------
# Analyzer
# ---------------------------------------------------------------------------


class RuleBasedTaskAnalyzer:
    """Pure rule-based task analyzer. No model calls, fully deterministic."""

    def analyze(
        self,
        raw_request: str,
        request_source: str = "test_fixture",
        repo_context: str | None = None,
        user_constraints: tuple[str, ...] = (),
        target_repo_metadata: dict[str, Any] | None = None,
    ) -> TaskAnalysis:
        text = raw_request.lower().strip()
        positive_text = _positive_risk_text(text)

        domain = self._classify_domain(text)
        intent = self._classify_intent(text)
        risk_flags, pos_evidence, neg_evidence = self._detect_risk_flags(text, positive_text)
        cognitive, context, exec_risk, ambiguity = self._compute_complexity(
            text, domain, intent, risk_flags
        )
        complexity_score = (
            COMPLEXITY_WEIGHTS["cognitive"] * cognitive
            + COMPLEXITY_WEIGHTS["context"] * context
            + COMPLEXITY_WEIGHTS["execution_risk"] * exec_risk
            + COMPLEXITY_WEIGHTS["ambiguity"] * ambiguity
        )
        context_budget, execution_budget = self._estimate_budgets(domain, intent, text)
        confidence, confidence_label, uncertainty_reasons = self._assess_confidence(
            domain, intent, text, risk_flags
        )
        risk_level = self._derive_risk_level(risk_flags, domain, intent)
        quality_req = self._derive_quality_requirement(text, risk_level)
        safe_default = self._determine_safe_default(confidence, risk_level)
        escalation = self._determine_escalation(confidence, risk_level, risk_flags)
        capabilities = self._detect_capabilities(text, domain, intent)
        features = self._detect_features(text, domain, intent, risk_flags)

        return TaskAnalysis(
            analysis_id=f"analysis-{uuid.uuid4().hex[:12]}",
            raw_request_snapshot=raw_request,
            request_source=request_source,
            primary_task_type=f"{domain}_{intent}",
            task_domain=domain,
            task_intent=intent,
            risk_flags=risk_flags,
            complexity_score=round(complexity_score, 4),
            cognitive_complexity=round(cognitive, 4),
            context_complexity=round(context, 4),
            execution_risk=round(exec_risk, 4),
            ambiguity_score=round(ambiguity, 4),
            required_capabilities=capabilities,
            context_budget_estimate=context_budget,
            execution_budget_estimate=execution_budget,
            quality_requirement=quality_req,
            risk_level=risk_level,
            confidence=round(confidence, 4),
            confidence_label=confidence_label,
            uncertainty_reason=uncertainty_reasons,
            safe_default=safe_default,
            escalation_trigger=escalation,
            positive_evidence=pos_evidence,
            negative_evidence=neg_evidence,
            features_detected=features,
            analysis_method="rule_only",
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    def _classify_domain(self, text: str) -> str:
        # Priority-based: check strong signals first
        if "architecture" in text or "microservice" in text or "system design" in text:
            return "architecture"
        if "calculate" in text or "batch size" in text or "formula" in text:
            return "math"
        if any(ext in text for ext in (".py", ".js", ".ts", ".go", ".rs", ".java", "test_auth")):
            return "code"
        if any(kw in text for kw in ("bug", "fix", "debug", "function", "class", "module")):
            return "code"
        if any(kw in text for kw in ("readme", "documentation", "docs", "document")):
            return "docs"
        if any(kw in text for kw in ("config", "configuration", "settings", "ci/cd", "yaml", ".env")):
            return "config"
        if any(kw in text for kw in ("deploy", "docker", "kubernetes", "k8s", "terraform", "infrastructure", "deployment")):
            return "infra"
        if any(kw in text for kw in ("commit", "push", "merge", "branch", "git", "repo")):
            return "repo_ops"
        if any(kw in text for kw in ("audit", "compliance", "governance", "vulnerability", "security")):
            return "governance"
        # Fallback to scoring
        scores: dict[str, int] = {}
        for domain, keywords in _DOMAIN_KEYWORDS.items():
            scores[domain] = sum(1 for kw in keywords if kw in text)
        best = max(scores, key=scores.get)  # type: ignore[arg-type]
        return best if scores[best] > 0 else "other"

    def _classify_intent(self, text: str) -> str:
        # Priority-based: check strong signals first
        if "summarize" in text or "summary" in text:
            return "summarize"
        if "audit" in text:
            return "audit"
        if "debug" in text or ("fix" in text and ("bug" in text or "failing" in text)):
            return "debug"
        if "generate" in text or "create" in text or "build" in text:
            return "generate"
        if "plan" in text or "strategy" in text or "roadmap" in text:
            return "plan"
        if "review" in text or "inspect" in text or "examine" in text:
            return "review"
        if "refactor" in text or "restructure" in text:
            return "refactor"
        if "explain" in text or "describe" in text or "how does" in text:
            return "explain"
        if "compare" in text or "versus" in text or "contrast" in text:
            return "compare"
        # Fallback to scoring
        scores: dict[str, int] = {}
        for intent, keywords in _INTENT_KEYWORDS.items():
            scores[intent] = sum(1 for kw in keywords if kw in text)
        best = max(scores, key=scores.get)  # type: ignore[arg-type]
        return best if scores[best] > 0 else "classify"

    def _detect_risk_flags(
        self, text: str, positive_text: str
    ) -> tuple[tuple[str, ...], tuple[Evidence, ...], tuple[Evidence, ...]]:
        flags: list[str] = []
        pos_evidence: list[Evidence] = []
        neg_evidence: list[Evidence] = []

        for flag, keywords in _RISK_KEYWORDS.items():
            detected = False
            matched_kw = None
            matched_idx = -1

            for kw in keywords:
                idx = positive_text.find(kw)
                if idx >= 0 and not _is_negated_occurrence(text, kw, text.find(kw)):
                    detected = True
                    matched_kw = kw
                    matched_idx = idx
                    break

            if detected:
                flags.append(flag)
                pos_evidence.append(Evidence(
                    feature=flag,
                    text=matched_kw,
                    span=(matched_idx, matched_idx + len(matched_kw)),
                    polarity="positive",
                    source="raw_request",
                    rule_id=f"risk_{flag}",
                    confidence=0.9,
                ))
            else:
                neg_evidence.append(Evidence(
                    feature=flag,
                    text="[negated]",
                    span=(0, 0),
                    polarity="negative",
                    source="raw_request",
                    rule_id=f"negation_{flag}",
                    confidence=0.95,
                    negation_scope=f"negation phrase suppressed {flag}",
                ))

        return tuple(flags), tuple(pos_evidence), tuple(neg_evidence)

    def _compute_complexity(
        self, text: str, domain: str, intent: str, risk_flags: tuple[str, ...]
    ) -> tuple[float, float, float, float]:
        # Cognitive complexity: reasoning-heavy domains/intents
        cognitive = 0.2
        if domain in ("architecture", "math", "code"):
            cognitive += 0.3
        if intent in ("debug", "plan", "refactor", "generate"):
            cognitive += 0.2
        if "multi-step" in text or "trade-off" in text or "tradeoff" in text:
            cognitive += 0.2
        cognitive = min(cognitive, 1.0)

        # Context complexity: input size indicators
        context = 0.1
        if "code" in text and ("block" in text or "file" in text or "module" in text):
            context += 0.2
        if "500-file" in text or "large codebase" in text or "entire repo" in text:
            context += 0.4
        if "multi-file" in text or "cross-module" in text:
            context += 0.3
        if len(text) > 500:
            context += 0.1
        context = min(context, 1.0)

        # Execution risk: from risk flags
        exec_risk = 0.0
        high_risk_flags = {"target_write", "provider_call", "sandbox_execution", "deployment", "destructive_operation"}
        for flag in risk_flags:
            if flag in high_risk_flags:
                exec_risk += 0.25
            elif flag == "secret_handling":
                exec_risk += 0.3
            else:
                exec_risk += 0.1
        exec_risk = min(exec_risk, 1.0)

        # Ambiguity: vague requests, missing criteria
        ambiguity = 0.1
        vague_phrases = ("make it better", "improve", "optimize", "somehow", "whatever")
        ambiguity += sum(0.15 for vp in vague_phrases if vp in text)
        if "unclear" in text or "ambiguous" in text:
            ambiguity += 0.2
        if len(text.split()) < 5:
            ambiguity += 0.2
        ambiguity = min(ambiguity, 1.0)

        return cognitive, context, exec_risk, ambiguity

    def _estimate_budgets(self, domain: str, intent: str, text: str) -> tuple[int, int]:
        base = _BUDGET_BASE.get(domain, 2000)
        multiplier = _INTENT_MULTIPLIER.get(intent, 1.0)
        context_budget = int(base * multiplier)
        execution_budget = int(base * multiplier * 0.75)

        # Check for explicit budget constraint
        if "500 tokens" in text or "budget" in text:
            context_budget = min(context_budget, 500)
            execution_budget = min(execution_budget, 375)

        return context_budget, execution_budget

    def _assess_confidence(
        self, domain: str, intent: str, text: str, risk_flags: tuple[str, ...]
    ) -> tuple[float, str, tuple[str, ...]]:
        confidence = 0.8
        reasons: list[str] = []

        if domain == "other":
            confidence -= 0.2
            reasons.append("domain_unclear")
        if intent == "classify":
            confidence -= 0.15
            reasons.append("intent_unclear")
        if len(text.split()) < 5:
            confidence -= 0.2
            reasons.append("request_too_short")
        if "ambiguous" in text or "unclear" in text:
            confidence -= 0.15
            reasons.append("explicit_ambiguity")
        if "high_uncertainty" in risk_flags:
            confidence -= 0.1
            reasons.append("high_uncertainty_flag")

        confidence = max(0.0, min(1.0, confidence))

        if confidence >= 0.7:
            label = "high"
        elif confidence >= 0.4:
            label = "medium"
        else:
            label = "low"

        return confidence, label, tuple(reasons)

    def _derive_risk_level(
        self, risk_flags: tuple[str, ...], domain: str, intent: str
    ) -> str:
        if any(f in risk_flags for f in ("destructive_operation", "secret_handling", "deployment")):
            return "critical"
        if any(f in risk_flags for f in ("target_write", "provider_call", "sandbox_execution")):
            return "high"
        if len(risk_flags) >= 2:
            return "medium"
        if domain in ("governance", "infra") or intent == "audit":
            return "medium"
        return "low"

    def _derive_quality_requirement(self, text: str, risk_level: str) -> str:
        if "critical" in text or "production-grade" in text or "must be" in text:
            return "critical"
        if risk_level in ("critical", "high"):
            return "high"
        if "high quality" in text or "thorough" in text:
            return "high"
        if "quick" in text or "draft" in text or "rough" in text:
            return "draft"
        return "standard"

    def _determine_safe_default(self, confidence: float, risk_level: str) -> str:
        if confidence < 0.4:
            return "escalate_to_human"
        if risk_level in ("critical", "high"):
            return "noop_with_review"
        return "proceed_with_caution"

    def _determine_escalation(
        self, confidence: float, risk_level: str, risk_flags: tuple[str, ...]
    ) -> str | None:
        if confidence < 0.3:
            return "low_confidence"
        if risk_level == "critical":
            return "critical_risk"
        if "target_write" in risk_flags and "provider_call" in risk_flags:
            return "combined_boundary_risk"
        return None

    def _detect_capabilities(
        self, text: str, domain: str, intent: str
    ) -> tuple[str, ...]:
        caps: list[str] = []
        if domain == "code":
            caps.append("code_analysis")
        if intent in ("generate", "refactor"):
            caps.append("code_generation")
        if intent == "debug":
            caps.append("error_diagnosis")
        if domain == "math":
            caps.append("mathematical_reasoning")
        if domain == "architecture":
            caps.append("system_design")
        if "security" in text or "vulnerability" in text:
            caps.append("security_analysis")
        if "test" in text:
            caps.append("test_generation")
        return tuple(caps)

    def _detect_features(
        self, text: str, domain: str, intent: str, risk_flags: tuple[str, ...]
    ) -> dict[str, Any]:
        return {
            "domain": domain,
            "intent": intent,
            "has_code_blocks": "```" in text,
            "has_file_refs": any(kw in text for kw in (".py", ".js", ".ts", ".yaml", ".json")),
            "risk_flag_count": len(risk_flags),
            "word_count": len(text.split()),
        }


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _positive_risk_text(text: str) -> str:
    """Return text with common negated boundary phrases removed."""
    result = text
    for phrase in _NEGATED_RISK_PHRASES:
        result = result.replace(phrase, " ")
    return result


_NEGATION_PREFIXES: tuple[str, ...] = (
    "without any ", "without ", "no ", "do not ", "don't ",
    "never ", "cannot ", "can't ", "must not ", "shall not ",
)


def _is_negated_occurrence(text: str, keyword: str, start: int) -> bool:
    """Check if a keyword occurrence at `start` is preceded by a negation prefix
    within the same clause (up to 40 chars or nearest conjunction/punctuation)."""
    clause_start = max(0, start - 40)
    clause = text[clause_start:start]
    for prefix in _NEGATION_PREFIXES:
        if prefix in clause:
            return True
    return False
