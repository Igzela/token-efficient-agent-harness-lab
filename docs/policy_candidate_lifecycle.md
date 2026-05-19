# Policy Candidate Lifecycle Track

## Overview

Defines the full lifecycle of a policy candidate from proposal through evidence
collection, approval, rollback planning, and policy registry activation.

## Lifecycle Stages

```
policy_candidate_manifest
  -> candidate_evidence_pack
    -> approval_record
      -> rollback_plan
        -> policy_registry_entry (status=active)
```

## Schemas

### policy_candidate_manifest (v1)

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | `policy_candidate.v1` |
| `candidate_id` | string | Unique identifier |
| `candidate_type` | string | One of the candidate_type enums |
| `title` | string | Human-readable title |
| `rationale` | string | Why this candidate exists |
| `source_refs` | list | Evidence sources |
| `proposed_change_summary` | string | What changes |
| `affected_components` | list | Components affected |
| `expected_benefit` | string | Expected improvement |
| `expected_risk` | string | Known risks |
| `required_evidence` | string | What evidence is needed |
| `evaluation_plan` | string | How to evaluate |
| `rollback_plan_ref` | string | Reference to rollback plan |
| `approval_required` | bool | Must be `true` |
| `created_at` | string | ISO timestamp |

### candidate_evidence_pack (v1)

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | `candidate_evidence.v1` |
| `candidate_id` | string | Links to manifest |
| `admitted_evidence_refs` | list | Evidence admitted for scoring |
| `diagnostic_evidence_refs` | list | Diagnostic-only evidence |
| `fixture_results` | list | Test results |
| `quality_deltas` | dict | Quality changes |
| `cost_deltas` | dict | Cost changes |
| `failure_cluster_refs` | list | Failure clusters |
| `human_review_refs` | list | Human review items |
| `recommendation` | string | One of the recommendation enums |

### approval_record (v1)

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | `approval_record.v1` |
| `candidate_id` | string | Links to manifest |
| `approver` | string | Approver identity |
| `decision` | string | approved / rejected / deferred |
| `rationale` | string | Decision rationale |
| `required_followups` | list | Required follow-up actions |
| `deployment_scope` | string | Scope of deployment |
| `rollback_required` | bool | Must be `true` |
| `approved_at` | string | ISO timestamp |

### rollback_plan (v1)

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | `rollback_plan.v1` |
| `rollback_plan_id` | string | Unique identifier |
| `candidate_id` | string | Links to manifest |
| `policy_id` | string | Policy to roll back |
| `rollback_scope` | string | One of the rollback_scope enums |
| `trigger_conditions` | list | When to trigger |
| `impacted_refs` | list | What is affected |
| `rollback_steps` | list | Steps to execute |
| `validation_steps` | list | How to validate |
| `rollback_owner` | string | Owner |
| `max_rollback_time` | string | Time limit |
| `fallback_policy` | string | Fallback |
| `status` | string | One of the rollback_statuses |
| `created_at` | string | ISO timestamp |

### policy_registry_entry (v1)

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | `policy_registry.v1` |
| `policy_id` | string | Unique identifier |
| `candidate_id` | string | Links to manifest |
| `policy_type` | string | Type of policy |
| `status` | string | One of the registry_statuses |
| `active_scope` | string | Scope when active |
| `version` | string | Version string |
| `evidence_pack_ref` | string | Reference to evidence pack |
| `approval_ref` | string | Reference to approval |
| `rollback_plan_ref` | string | Reference to rollback plan |
| `activated_at` | string | When activated |
| `retired_at` | string | When retired |

## Key Rules

- Adoption requires at least one `admitted_evidence_refs`.
- Diagnostic-only evidence cannot drive adoption.
- `approval_required` must always be `true`.
- Active policy must have `approval_ref` and `rollback_plan_ref`.
- Rejected/deferred approval cannot activate policy.
- Rollback targets must be harness-level only, not user project files.
- Shadow routing recommendations are diagnostic-only evidence.
- unknown_error-related candidates require human review.

## Module

Helpers live in `src/harness_core/policy_candidate.py`:

```python
from harness_core.policy_candidate import (
    validate_policy_candidate_manifest,
    validate_candidate_evidence_pack,
    validate_approval_record,
    validate_rollback_plan,
    validate_policy_registry_entry,
    candidate_has_required_evidence,
    evidence_pack_is_adoptable,
    approval_allows_activation,
    rollback_plan_is_ready,
    can_activate_policy,
    should_reject_diagnostic_only_candidate,
    create_policy_registry_entry,
)
```
