# Usage Ledger / Cost-of-Pass Track

## Overview

Records per-eval-row token usage, cost, retry, and tool-call data, then
aggregates by `cost_of_pass_group` to enable cost-efficiency comparison
across eval runs.

## usage_ledger Schema

Version: `usage_ledger.v1`

| Field | Type | Constraint |
|-------|------|------------|
| `schema_version` | string | Always `usage_ledger.v1` |
| `run_id` | string | Unique run identifier |
| `case_id` | string | Eval case identifier |
| `input_tokens` | int | >= 0 |
| `output_tokens` | int | >= 0 |
| `cached_tokens` | int | >= 0, must not exceed input_tokens |
| `request_count` | int | >= 0 |
| `tool_call_count` | int | >= 0 |
| `retry_count` | int | >= 0 |
| `wall_clock_ms` | int | >= 0 |
| `estimated_cost` | number | >= 0 |
| `pass` | bool | true = success, false = failure |
| `cost_of_pass_group` | string | Four-segment format (see below) |
| `model_profile_id` | string | Can be empty/null for no_model fixtures |
| `context_pack_id` | string | Can be empty/null for offline fixtures |

## cost_of_pass_group Format

```
<eval_suite>/<task_family>/<variant_family>/<success_criterion>
```

Example: `real_world_eval/bugfix/formal_issue/passes_final_gate`

### Rules

- Only compare cost-of-pass within the **same** `cost_of_pass_group`.
- Different groups can show **trends** but cannot claim A is more efficient than B.
- `cost_of_pass = group_total_estimated_cost / group_success_count`
- When `group_success_count == 0`, `cost_of_pass` is **undefined** and must report failure.

## Key Constraints

- No negative values for token/cost/retry/tool-call/wall-clock fields.
- `cached_tokens` must not exceed `input_tokens`.
- No usage ledger → no routing optimization enabled.
- No cost-of-pass → no token-efficient improvement claims.
- Quality threshold must be met before cost optimization.

## Module

Helpers live in `src/harness_core/usage_ledger.py`:

```python
from harness_core.usage_ledger import (
    validate_usage_ledger_row,
    aggregate_cost_of_pass,
    group_usage_rows,
    compare_cost_groups,
    detect_invalid_cost_comparison,
    is_valid_cost_of_pass_group,
    parse_cost_of_pass_group,
    UsageLedgerRow,
    CostOfPassAggregate,
    ComparisonResult,
)
```
