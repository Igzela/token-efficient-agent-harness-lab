# Validator Suite Requirements

Source: harness_architecture_book_v0.7.4.1-canonical §7, docs/stage0/retrospectives/ §4

## Overview

The Validator Suite validates all Stage 1 artifacts against their schemas. Week 1 defines 8 validators with clear input/output/pass/fail rules.

---

## Validator 1: Events Schema Validator

**Purpose:** Validate that each event conforms to event.v1 schema.

**Input:** Event JSON object

**Required fields:**
| Field | Type | Rule |
|-------|------|------|
| `event_id` | string | format `evt_YYYYMMDD_NNNNNN` |
| `schema_version` | string | must be `"event.v1"` |
| `event_type` | string | must be in registered event type enum |
| `timestamp` | string | ISO-8601 format |
| `producer` | object | must have `component_id` and `component_type` |
| `correlation` | object | must have `batch_id` and at least one of `task_id`/`project_id` |
| `severity` | string | `"info"` or `"warn"` or `"error"` |
| `payload` | object | non-null |
| `idempotency_key` | string | non-empty |
| `parent_event_id` | string or null | null allowed for root events |

**Pass:** All required fields present with correct types and values.
**Fail:** Any field missing, wrong type, or invalid value.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V1.1 | Stage 0 events.jsonl line 1 (valid event) | PASS |
| TC-V1.2 | Event missing `event_id` | FAIL |
| TC-V1.3 | Event with `schema_version: "v1"` (wrong) | FAIL |
| TC-V1.4 | Event with `severity: "critical"` (invalid) | FAIL |

---

## Validator 2: completion.json Validator

**Purpose:** Validate task completion records.

**Input:** completion.json object

**Required fields:**
| Field | Type | Rule |
|-------|------|------|
| `_template` | boolean | `false` for real completions |
| `node_id` | string | non-empty |
| `task_id` | string | non-empty |
| `status` | string | `"completed"` or `"failed"` |
| `exit_code` | int | `0` for success, non-zero for failure |
| `completion_type` | string | `"success"` or `"failure"` |
| `verifier_status` | string | `"pass"` or `"fail"` or `"skipped_manual"` or `"passed_on_retry"` |
| `write_claims_released` | boolean | must be `true` for completed tasks |
| `retry_count` | int | >= 0 |
| `finished_at` | string | ISO-8601 |

**Pass:** All required fields present with correct types. `_template` is `false`.
**Fail:** Any field missing, wrong type, or `_template` is `true`.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V2.1 | Stage 0 task-005 completion.json | PASS |
| TC-V2.2 | completion.json with `_template: true` | FAIL |
| TC-V2.3 | completion.json missing `exit_code` | FAIL |
| TC-V2.4 | completion.json with `status: "running"` | FAIL |

---

## Validator 3: handoff_pack Validator

**Purpose:** Validate handoff packs produced by tasks.

**Input:** handoff_pack.json object

**Required fields:**
| Field | Type | Rule |
|-------|------|------|
| `structured_fields` | object | must contain `status`, `completion_type`, `exit_code`, `artifacts_produced`, `duration_estimate`, `board_writeback_status` |
| `summary` | string | non-empty |
| `evidence_refs` | array | non-empty, each ref has `type` and `path` |

**Pass:** All three top-level fields present and non-empty.
**Fail:** Any field missing or empty.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V3.1 | Stage 0 task-005 handoff_pack.json | PASS |
| TC-V3.2 | handoff_pack missing `evidence_refs` | FAIL |
| TC-V3.3 | handoff_pack with empty `summary` | FAIL |
| TC-V3.4 | handoff_pack with `structured_fields` missing `exit_code` | FAIL |

---

## Validator 4: approval_request Validator

**Purpose:** Validate approval request templates.

**Input:** approval_request object

**Required fields:**
| Field | Type | Rule |
|-------|------|------|
| `approval_id` | string | non-empty |
| `task_id` | string | non-empty |
| `risk_level` | string | non-empty |
| `requested_action` | string | must be in enum: `modify_files`, `delete_files`, `run_command`, `submit_pr`, `use_paid_api`, `access_external_service` |
| `summary` | string | non-empty |
| `reason` | string | non-empty |
| `affected_files` | array | non-empty |
| `options` | array | non-empty, each in enum: `approve`, `reject`, `approve_readonly_only`, `approve_with_constraints`, `defer` |
| `timeout_policy` | string or object | non-empty |
| `created_at` | string | ISO-8601 |

**Optional fields:**
- `decision`: `"pending"` | `"approved"` | `"rejected"` (default: `"pending"`)
- `diff_preview`: string
- `cost_estimate`: string
- `risk_notes`: string
- `expires_at`: string

**Pass:** All required fields present with correct types. `decision` defaults to `"pending"` if absent.
**Fail:** Any required field missing or wrong type.

**Key semantic:** `decision: "pending"` means the template is valid but the approval action has NOT been executed. This is a valid state — it does NOT fail validation.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V4.1 | Stage 0 task-004 approval_request (decision: pending) | PASS |
| TC-V4.2 | approval_request missing `timeout_policy` | FAIL |
| TC-V4.3 | approval_request with `requested_action: "delete_database"` (invalid enum) | FAIL |
| TC-V4.4 | approval_request with `decision: "pending"` | PASS (pending is valid) |

---

## Validator 5: Advisor Protocol Validator

**Purpose:** Validate individual advisor call records.

**Input:** Single advisor call record (from task events or run_log)

**Required fields per call:**
| Field | Type | Rule |
|-------|------|------|
| `call_type` | string | `"preflight"` or `"correction"` or `"checkpoint"` or `"stuck"` or `"arbitration"` or `"risk_scan"` |
| `diagnosis` | string | non-empty |
| `recommended_action` | string | non-empty |
| `do_not_do` | string or array | non-empty |
| `confidence` | float | 0.0–1.0 |

**Pass:** All required fields present with correct types and values.
**Fail:** Any field missing, wrong type, or out of range.

**Call count rule:**
- The validator validates each individual call's schema.
- Expected advisor call count is task-specific, NOT a global rule.
- For the Stage 0 task-005 fixture, `expected_min_advisor_calls = 2` (Preflight + Correction).
- Other tasks may have 0, 1, or more advisor calls as needed.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V5.1 | Stage 0 task-005 advisor preflight call | PASS |
| TC-V5.2 | Stage 0 task-005 advisor correction call | PASS |
| TC-V5.3 | Advisor call missing `diagnosis` | FAIL |
| TC-V5.4 | Advisor call with `confidence: 1.5` (out of range) | FAIL |
| TC-V5.5 | Advisor record with only 1 call, task fixture requires 2 → FAIL | FAIL (count check against fixture requirement) |

---

## Validator 6: failure_code Enum Validator

**Purpose:** Validate that failure codes use the canonical enum.

**Input:** `failure_code` string and optional `failure_subcode` string

**Canonical primary codes:**
```
F001_TIMEOUT
F002_BUDGET_EXCEEDED
F003_DEPENDENCY_FAILED
F004_APPROVAL_REJECTED
F005_PROVIDER_UNAVAILABLE
F006_SCOPE_VIOLATION
F007_TEST_FAILURE
F008_FORMAT_ERROR
F009_POLICY_VIOLATION
F010_CANCELLED
```

**Rules:**
- Primary `failure_code` MUST be in the canonical enum
- `failure_subcode` is freeform (task-specific detail)
- Primary code is the dot-separated first segment if subcode is included (e.g., `F008.handoff_pack_incomplete`)

**Pass:** Primary code is in the canonical enum.
**Fail:** Primary code is not in the canonical enum.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V6.1 | `failure_code: "F008_FORMAT_ERROR"` | PASS |
| TC-V6.2 | `failure_code: "some_random_string"` | FAIL |
| TC-V6.3 | `failure_code: "F008_FORMAT_ERROR"`, `failure_subcode: "handoff_pack_incomplete"` | PASS |
| TC-V6.4 | `failure_code: "FORMAT_ERROR"` (missing F-prefix) | FAIL |

---

## Validator 7: allowed_files Completeness Checker

**Purpose:** Check that an item's `allowed_files` includes all files the task will actually need.

**Input:** item definition (from project_board.md) + list of required files

**Required file patterns:**
- `events.jsonl` — if task writes events
- `completion.json` — if task produces completion
- `handoff_pack.json` — if task produces handoff
- `project_board.md` — if task does status writeback
- `run_log.md` — always required
- `retrospective.md` — always required

**Pass:** All required files are in `allowed_files`.
**Fail:** Any required file missing from `allowed_files`.

**Stage 0 reference failures:**
| Item | Original allowed_files | Actual needed | Gap |
|------|----------------------|---------------|-----|
| item_002 | 2 files | 7 files | 5 task_spec.json missing |
| item_003 | 2 files | 5 files | events.jsonl, completion.json, project_board.md missing |
| item_004 | 2 files | 8 files | events.jsonl, completion.json, project_board.md, etc. missing |
| item_005 | 3 files | 9 files | handoff_pack.json, project_board.md, etc. missing |

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V7.1 | Stage 0 item_005 current allowed_files (9 files) | PASS |
| TC-V7.2 | Stage 0 item_002 original allowed_files (2 files) | FAIL |
| TC-V7.3 | Stage 0 item_003 original allowed_files (2 files) | FAIL |
| TC-V7.4 | Stage 0 item_004 original allowed_files (2 files) | FAIL |

---

## Validator 8: Replay Preflight Checker

**Purpose:** Validate an event stream before projection replay.

**Input:** Array of lines from an events.jsonl file

**Checks:**
1. Every line is valid JSON
2. Every event has `schema_version = "event.v1"`
3. No duplicate `event_id`
4. Timestamps are non-decreasing (warning if not)
5. All `parent_event_id` references exist in the stream
6. Every line ends with `\n`
7. No line contains multiple JSON objects

**Pass:** All checks pass (warnings allowed).
**Fail:** Any check produces an error.

**Test cases:**
| # | Input | Expected |
|---|-------|----------|
| TC-V8.1 | Stage 0 events.jsonl (original, with line 17 issues) | FAIL (line 17 concatenation + duplicate event_id) |
| TC-V8.2 | Sanitized copy with line 17 split and duplicate removed | PASS |
| TC-V8.3 | Stage 0 task-005 events.jsonl (10 events) | PASS |
| TC-V8.4 | Events with non-decreasing timestamps | WARNING (not fail) |
| TC-V8.5 | Event with non-existent parent_event_id | WARNING or FAIL |

---

## Aggregate Validation

```
validator.validate_all({
  events: event_stream,
  completion: completion_json,
  handoff_pack: handoff_pack_json,
  approval_request: approval_request_json,
  advisor_calls: advisor_records,
  failure_code: failure_code_string,
  allowed_files: item_allowed_files,
}) → AggregateResult

AggregateResult:
  overall: "pass" | "fail"
  results: Map<validator_name, ValidationResult>
  errors: ErrorDetail[]
  warnings: WarningDetail[]
```

## Canonical failure_code Enum

```
F001_TIMEOUT
F002_BUDGET_EXCEEDED
F003_DEPENDENCY_FAILED
F004_APPROVAL_REJECTED
F005_PROVIDER_UNAVAILABLE
F006_SCOPE_VIOLATION
F007_TEST_FAILURE
F008_FORMAT_ERROR
F009_POLICY_VIOLATION
F010_CANCELLED
```
