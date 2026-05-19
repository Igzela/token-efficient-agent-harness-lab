# Governance Approval Path Enforcement Track

## Overview

Ensures policy candidates only reach active status after passing all governance
gates. Governance only decides — it never executes deployment.

## Lifecycle

```
candidate + evidence + approval + rollback + registry
  -> governance evaluation (all gates)
    -> governance_decision
      -> human/tool executes registry update (if approve_activation)
```

## governance_decision Schema

Version: `governance_decision.v1`

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | `governance_decision.v1` |
| `decision_id` | string | Unique identifier |
| `candidate_id` | string | Links to manifest |
| `policy_id` | string | Policy identifier |
| `decision` | string | One of the decision enums |
| `decision_basis` | object | Evidence, approval, rollback refs |
| `gate_results` | object | Pass/fail for each gate |
| `blocked_reasons` | list | Why activation is blocked |
| `allowed_next_actions` | list | What can happen next |
| `forbidden_next_actions` | list | What cannot happen next |
| `decided_by` | string | Who/what decided |
| `decided_at` | string | ISO timestamp |

### decision enum

- `approve_activation` — all gates pass
- `reject_activation` — hard block
- `defer_activation` — needs more time/info
- `require_more_evidence` — evidence insufficient

### gate_results

| Gate | Pass Condition |
|------|----------------|
| `evidence_gate` | `admitted_evidence_refs` non-empty |
| `approval_gate` | `approval_record.decision` = approved |
| `rollback_gate` | `rollback_plan.status` = approved, steps non-empty |
| `scope_gate` | No user project file paths in impacted_refs |
| `unknown_error_gate` | No unknown_error evidence, or has human_review_refs |

## Key Rules

- **approve_activation** only when ALL gates pass.
- **Diagnostic-only** evidence fails evidence_gate.
- **Rejected/deferred** approval fails approval_gate.
- **Rollback plan** must be approved (not just proposed).
- **Scope gate** blocks user project file paths.
- **Unknown error** evidence requires human review.
- **Governance never modifies** policy_registry directly.
- **Activation** remains offline/schema-only.

## Module

Helpers live in `src/harness_core/governance.py`:

```python
from harness_core.governance import (
    validate_governance_decision,
    evaluate_evidence_gate,
    evaluate_approval_gate,
    evaluate_rollback_gate,
    evaluate_scope_gate,
    evaluate_unknown_error_gate,
    decide_policy_activation,
    governance_allows_activation,
    governance_blocks_activation,
    explain_blocked_reasons,
)
```
