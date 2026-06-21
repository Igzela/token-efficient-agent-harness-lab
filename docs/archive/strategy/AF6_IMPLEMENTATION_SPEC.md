# AF-6 Implementation Brief

Status: implementation brief for the AF-6 Auto Fusion track.

This file turns the AF-6 boundary reset into a concrete slice order for coding agents. It does not replace `docs/NEXT_DECISION.md`; it expands the AF-6 section there.

## Goal

After AF-6 is complete, the system should be able to:

- generate provider/model candidate plans automatically;
- execute fusion panels with bounded parallelism;
- turn completed adaptive executions into learning observations;
- run continuous bounded experiments;
- promote better routing policies from evidence;
- expose a completion-style API;
- optionally route eligible completion traffic through adaptive routing after AF-6 enablement.

AF-6 is provider/model routing work only. It does not grant repository output, release, deploy, or merge authority.

## Required PR sequence

Implement in this order:

```text
AF-6A candidate generator
AF-6B parallel panel execution
AF-6C online observation capture
AF-6D continuous experiments
AF-6E evidence-based auto promotion
AF-6F completion API and optional default adaptive routing
AF-6G docs/runbook/status cleanup
```

Each slice should be a separate PR. AF-6 implementation PRs are not auto-merge eligible.

## AF-6A Candidate generator

Add deterministic generation of executable candidates from configured endpoints.

Candidate kinds:

- single endpoint
- ordered fallback
- fusion: panel, judge, synthesizer

Required candidate fields:

```text
candidate_id
candidate_hash
task_class
objective
candidate_kind
member_endpoint_ids
plan
estimated_cost_usd
estimated_tokens
estimated_latency_ms
required_capabilities
registry_snapshot_hash
```

Rules:

- generation is pure and deterministic;
- generation does not call providers;
- candidates are capped by config;
- invalid, duplicate, unavailable, over-budget, or capability-missing endpoints are rejected;
- tests cover deterministic IDs, ordering, caps, and rejection cases.

Likely files:

```text
engine/src/feedback/adaptive_candidate.rs
engine/src/feedback/mod.rs
engine/tests/test_adaptive_candidate_generation.rs
```

## AF-6B Parallel panel execution

Extend fusion execution so panel calls can run concurrently under a configured cap. Judge and synthesizer remain ordered after the panel stage.

Required behavior:

- parallelism is bounded;
- partial panel failure policy is explicit;
- cost, token, call, timeout, identity, redaction, audit, and kill behavior remain enforced;
- serial behavior remains compatible.

Likely files:

```text
engine/src/provider/adaptive_execution.rs
engine/tests/test_adaptive_fusion_execution.rs
```

## AF-6C Online observation capture

Persist safe summaries from adaptive executions.

Observation fields:

```text
observation_id
run_id
request_id
task_class
objective
risk_level
candidate_id
candidate_hash
policy_hash
candidate_kind
success
quality_score
quality_score_source
tool_success_score
cost_usd
latency_ms
input_tokens
output_tokens
created_at
```

Rules:

- observations store summaries, not transcripts;
- duplicate run/candidate observations are idempotent or rejected;
- malformed observations are rejected with a reason;
- observations feed existing scoring.

Likely files:

```text
engine/src/storage/local_product_store/adaptive_observation.rs
engine/src/storage/local_product_store/mod.rs
engine/src/provider/adaptive_execution.rs
engine/src/http_server/handlers/workflow_runs.rs
engine/tests/test_adaptive_observation_capture.rs
```

## AF-6D Continuous experiments

Add controlled traffic allocation to candidate experiments.

Required behavior:

- disabled unless AF-6 experiment gates are enabled;
- deterministic request bucketing;
- configurable traffic percentage;
- risk, budget, token, time, call, and concurrency limits apply;
- experiment results create observations;
- pause/kill controls exist.

Likely files:

```text
engine/src/feedback/adaptive_experiment.rs
engine/src/feedback/contextual_policy.rs
engine/src/provider/adaptive_execution.rs
engine/tests/test_adaptive_online_experiments.rs
```

## AF-6E Evidence-based auto promotion

Promote stronger policies when evidence thresholds pass.

Required behavior:

- disabled unless AF-6 promotion gates are enabled;
- uses minimum samples, confidence, quality delta, cost delta, latency delta, and failure-rate guard;
- stores policy snapshot before activation;
- stores previous policy hash for rollback;
- supports rollout percentage;
- blocks stale or missing evidence.

Likely files:

```text
engine/src/storage/local_product_store/adaptive_policy.rs
engine/src/feedback/contextual_policy.rs
engine/tests/test_adaptive_auto_promotion.rs
```

## AF-6F Completion API and optional default routing

Add:

```text
POST /api/v1/adaptive-fusion/completions
```

Request fields:

```text
prompt
task_class optional
objective optional
risk_level optional
metadata optional
include_routing_metadata optional
```

Response fields:

```text
schema_version
output
candidate_id
policy_hash optional
observation_id optional
routing_metadata optional
usage
```

Required behavior:

- endpoint uses existing auth and execution gates;
- compact response hides orchestration details by default;
- operator metadata is available when requested or through audit APIs;
- optional `/api/v1/dispatch` delegation remains off unless AF-6 default-routing gate is enabled;
- target-output workflows are not affected.

Likely files:

```text
engine/src/http_server/routes.rs
engine/src/http_server/handlers/adaptive_completions.rs
engine/src/http_server/mod.rs
sdk/typescript/src/api-types.ts
sdk/typescript/src/index.ts
dashboard/src/lib/api-client.ts
engine/tests/test_http_server.rs
sdk/typescript/tests/client.test.mjs
```

## Cross-slice checks

Every implementation PR must keep:

- deterministic exposed IDs and hashes;
- bounded cost, tokens, calls, latency, and concurrency;
- provider/model identity checks;
- redaction and capped outputs;
- audit events for selection, call, result, promotion, rollback, and kill;
- rollback path for promoted policy;
- full relevant tests.

## Required verification

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets -- -D warnings
cargo test -p engine --test test_adaptive_fusion_execution
cargo test -p engine --test test_contextual_adaptive_policy
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

## Final acceptance

AF-6 is done only when all six implementation slices are merged, CI is green, docs/status/runbook match behavior, and the completion API can exercise adaptive candidate generation, execution, observation, experiment, and promotion flows under AF-6 gates.
