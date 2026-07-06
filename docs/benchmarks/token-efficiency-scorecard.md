# Token Efficiency Scorecard

## Purpose

The token-efficiency scorecard is the normalized evidence contract for comparing native harness runs against external agent runtime baselines.

It must answer three questions separately:

1. Did the task pass?
2. How many tokens, tool calls, retries, and repeated reads were used?
3. Which context-control behavior explains the result?

A run that saves tokens but fails the task is not a successful token-efficiency improvement. A run that passes but hides raw traces, failures, or repeated reads is not valid benchmark evidence.

## Scope

This document defines the scorecard shape only. It does not implement a database migration, API endpoint, dashboard view, or external runtime adapter.

## Run-Level Record

Logical record name:

```text
runtime_adapter_runs
```

Minimum fields:

```text
adapter_run_id              stable run id
schema_version              token_efficiency_scorecard.v1
runtime_kind                native_harness | langgraph | crewai | microsoft_agent_framework | other
runtime_version             external runtime version or native commit/ref
adapter_version             adapter or importer version
scenario_id                 benchmark scenario id
mode                        native_control_plane | stateless_reread | stateful_store | pruned_context | external_runtime
state_strategy              none | full_history | durable_state | memory_digest | retrieval_refs | mixed
started_at                  RFC3339 timestamp
finished_at                 RFC3339 timestamp or null
status                      pass | fail | error | blocked
pass_fail_reason            bounded explanation, no raw transcript
quality_score               optional numeric score, 0.0 to 1.0
quality_method              rule | test | human_review | model_judge | mixed | none
input_token_total           total prompt/input tokens
output_token_total          total completion/output tokens
context_token_total         context injected or read by model steps
repeated_context_token_total token estimate for repeated rereads
retrieved_ref_token_total   token estimate from retrieval/context refs
tool_call_count             total tool calls
redundant_tool_call_count   duplicate or repeated tool calls
retry_count                 retries or repair loops
step_count                  normalized step count
duration_ms                 wall-clock duration
estimated_cost_usd          optional if pricing configured
raw_trace_artifact_id       app-owned bounded artifact reference
redaction_status            not_needed | redacted | rejected
created_at                  RFC3339 timestamp
```

## Step-Level Record

Logical record name:

```text
runtime_adapter_steps
```

Minimum fields:

```text
adapter_step_id             stable step id
adapter_run_id              parent run id
step_index                  zero-based order
node_name                   runtime node or normalized step name
agent_role                  planner | executor | reviewer | evaluator | unknown
operation_kind              model_call | tool_call | state_read | state_write | retrieval | evaluation | control
input_tokens                step input tokens
output_tokens               step output tokens
context_tokens              context tokens available to the step
repeated_context_tokens     repeated context estimate for this step
retrieved_refs_count        number of retrieval/context refs
retrieved_ref_tokens        token estimate from retrieval/context refs
tool_name                   tool name or null
tool_call_id                source tool call id or null
status                      pass | fail | error | skipped
error_kind                  bounded error class, no raw secret-bearing output
state_read_bytes            bytes read from durable state, if known
state_write_bytes           bytes written to durable state, if known
started_at                  RFC3339 timestamp
finished_at                 RFC3339 timestamp or null
```

## Derived Metrics

Scorecards should compute these derived values when source data permits:

```text
total_tokens = input_token_total + output_token_total
context_share = context_token_total / max(total_tokens, 1)
repeated_context_ratio = repeated_context_token_total / max(context_token_total, 1)
tool_redundancy_ratio = redundant_tool_call_count / max(tool_call_count, 1)
tokens_per_passing_run = total_tokens if status == pass else null
cost_per_passing_run = estimated_cost_usd if status == pass else null
step_retry_ratio = retry_count / max(step_count, 1)
```

For stateful-versus-stateless benchmarks, the primary comparison is not only total tokens. The minimum comparison set is:

```text
pass rate
quality score
total input tokens
context tokens per step
repeated context ratio
tool redundancy ratio
retry count
duration
```

## Validity Rules

A scorecard is invalid if:

- the run status is missing;
- pass/fail is inferred only from lower token use;
- token estimates are mixed across incompatible sources without a `quality_method` or notes;
- raw prompts, raw outputs, transcripts, credentials, or secret-shaped values are persisted;
- tool failures are hidden;
- provider calls happen in CI;
- the adapter changes execution authority or target-output authority;
- external runtime traces cannot be tied back to a scenario id and runtime version.

## Minimal JSON Shape

```json
{
  "schema_version": "token_efficiency_scorecard.v1",
  "adapter_run_id": "run-example",
  "runtime_kind": "langgraph",
  "runtime_version": "pinned-or-recorded-version",
  "scenario_id": "iterative_debug_basic",
  "mode": "stateful_store",
  "state_strategy": "durable_state",
  "status": "pass",
  "quality_score": 0.9,
  "input_token_total": 12000,
  "output_token_total": 1800,
  "context_token_total": 9500,
  "repeated_context_token_total": 1100,
  "retrieved_ref_token_total": 2400,
  "tool_call_count": 18,
  "redundant_tool_call_count": 2,
  "retry_count": 1,
  "step_count": 9,
  "duration_ms": 42000,
  "raw_trace_artifact_id": "artifact-example",
  "redaction_status": "redacted"
}
```

## Implementation Notes

The first implementation should be importer-first:

1. accept a bounded trace summary;
2. validate scorecard fields;
3. store raw source trace only as a redacted app-owned artifact;
4. compute derived metrics;
5. expose read-only scorecards before adding execution or adapter runners.

Do not build a full external runtime runner until the scorecard contract is stable.
