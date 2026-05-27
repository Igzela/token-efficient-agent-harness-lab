"""Generate the 20 golden dispatch fixtures."""

import json
from pathlib import Path

FIXTURES = [
    {
        "fixture_id": "fixture_01",
        "name": "low_risk_summary",
        "raw_request": "Summarize the README file for the project",
        "expected_analysis": {
            "task_domain": "docs",
            "task_intent": "summarize",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_02",
        "name": "doc_audit",
        "raw_request": "Audit the documentation for broken links and outdated references",
        "expected_analysis": {
            "task_domain": "docs",
            "task_intent": "audit",
            "risk_flags": [],
            "risk_level": "medium",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_03",
        "name": "code_review",
        "raw_request": "Review auth.py for security issues and potential vulnerabilities",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "review",
            "risk_flags": [],
            "risk_level": "medium",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_04",
        "name": "code_gen",
        "raw_request": "Generate a CLI tool for YAML validation with error reporting",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "generate",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_05",
        "name": "debug",
        "raw_request": "Debug the failing test in test_auth.py and fix the root cause",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "debug",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_06",
        "name": "architecture",
        "raw_request": "Design the architecture for a new microservice with event-driven communication",
        "expected_analysis": {
            "task_domain": "architecture",
            "task_intent": "plan",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_07",
        "name": "math",
        "raw_request": "Calculate the optimal batch size for cost efficiency given token pricing",
        "expected_analysis": {
            "task_domain": "math",
            "task_intent": "generate",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_08",
        "name": "config_review",
        "raw_request": "Review CI/CD configuration for best practices and security",
        "expected_analysis": {
            "task_domain": "config",
            "task_intent": "review",
            "risk_flags": [],
            "risk_level": "medium",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_09",
        "name": "infra_deploy",
        "raw_request": "Review infrastructure deployment pipeline for reliability issues",
        "expected_analysis": {
            "task_domain": "infra",
            "task_intent": "review",
            "risk_flags": [],
            "risk_level": "medium",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_10",
        "name": "provider_boundary",
        "raw_request": "Call OpenAI API to analyze the codebase and generate insights",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "generate",
            "risk_flags": ["provider_call"],
            "risk_level": "high",
            "confidence_label": "high",
            "safe_default": "noop_with_review",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled", "risk", "boundary", "manual_review"],
    },
    {
        "fixture_id": "fixture_11",
        "name": "target_write",
        "raw_request": "Fix the bug and commit the changes to main branch",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "debug",
            "risk_flags": ["target_write"],
            "risk_level": "high",
            "confidence_label": "high",
            "safe_default": "noop_with_review",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled", "risk", "target_write", "manual_review"],
    },
    {
        "fixture_id": "fixture_12",
        "name": "secret_handling",
        "raw_request": "Rotate the API keys in the config files and update credentials",
        "expected_analysis": {
            "task_domain": "config",
            "task_intent": "generate",
            "risk_flags": ["secret_handling", "target_write"],
            "risk_level": "critical",
            "confidence_label": "high",
            "safe_default": "noop_with_review",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled", "risk", "target_write", "manual_review"],
    },
    {
        "fixture_id": "fixture_13",
        "name": "long_context",
        "raw_request": "Analyze this 500-file large codebase for architectural patterns and anti-patterns",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "review",
            "risk_flags": ["long_context"],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_14",
        "name": "ambiguous",
        "raw_request": "Make it better",
        "expected_analysis": {
            "task_domain": "other",
            "task_intent": "classify",
            "risk_flags": ["high_uncertainty"],
            "risk_level": "low",
            "confidence_label": "low",
            "safe_default": "escalate_to_human",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled", "confidence", "manual_review"],
    },
    {
        "fixture_id": "fixture_15",
        "name": "conflicting",
        "raw_request": "Minimize cost but use the most powerful model available for this task",
        "expected_analysis": {
            "task_domain": "other",
            "task_intent": "classify",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "medium",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_16",
        "name": "read_only_high_risk",
        "raw_request": "Audit the database schema for security vulnerabilities and compliance gaps",
        "expected_analysis": {
            "task_domain": "governance",
            "task_intent": "audit",
            "risk_flags": [],
            "risk_level": "medium",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_17",
        "name": "negated_no_write",
        "raw_request": "Review code with no target repo writes, read-only validation only",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "review",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_18",
        "name": "negated_no_execute",
        "raw_request": "Analyze deployment config without any provider calls or sandbox execution",
        "expected_analysis": {
            "task_domain": "config",
            "task_intent": "review",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_19",
        "name": "budget_constrained",
        "raw_request": "Summarize the docs within 500 tokens budget",
        "expected_analysis": {
            "task_domain": "docs",
            "task_intent": "summarize",
            "risk_flags": [],
            "risk_level": "low",
            "confidence_label": "high",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
    {
        "fixture_id": "fixture_20",
        "name": "high_quality_critical",
        "raw_request": "Critical security review of authentication system, must be production-grade",
        "expected_analysis": {
            "task_domain": "code",
            "task_intent": "review",
            "risk_flags": [],
            "risk_level": "medium",
            "confidence_label": "high",
            "quality_requirement": "critical",
            "safe_default": "proceed_with_caution",
        },
        "expected_gates": ["provider_disabled", "sandbox_disabled"],
    },
]


def main() -> None:
    out_dir = Path(__file__).parent
    for fixture in FIXTURES:
        path = out_dir / f"{fixture['fixture_id']}_{fixture['name']}.json"
        data = {
            "fixture_id": fixture["fixture_id"],
            "name": fixture["name"],
            "raw_request": fixture["raw_request"],
            "request_source": "test_fixture",
            "expected_analysis": fixture["expected_analysis"],
            "expected_gates": fixture["expected_gates"],
        }
        path.write_text(json.dumps(data, indent=2) + "\n")
        print(f"wrote {path.name}")


if __name__ == "__main__":
    main()
