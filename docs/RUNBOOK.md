# Agent Control Plane — Runbook

Operator procedures for the local Agent Control Plane.

Last updated: 2026-07-14.

## Agent Runtime and Tool Policy Operations

Agent Runtime is an engine workflow executor, not the GitHub/Vader repository-maintenance orchestrator. The Rust scheduler remains the sole owner of admission, leases, retries, cooldown, concurrency, pause/resume, and run state. A provider decision performs one call and returns one typed action; there is no hidden agent loop.

Keep live execution default-off. For an authenticated trusted-local provider-backed run, configure the existing symbolic provider credential boundary and cost gates, then require all of:

```bash
export ACP_REQUIRE_AUTH=1
export ACP_ADMIN_API_KEY="$SYMBOLIC_LOCAL_ADMIN_KEY"
export ACP_ENABLE_PROVIDER_EXECUTION=1
export ACP_ENABLE_AGENT_RUNTIME=1
export ACP_AGENT_RUNTIME_KILL_SWITCH=0
export ACP_AGENT_MAX_CONCURRENT_GLOBAL=2
export ACP_AGENT_MAX_CONCURRENT_PER_RUN=1
export ACP_SCHEDULER_MAX_RETRIES=1
export ACP_SCHEDULER_LEASE_TIMEOUT_MS=302000
```

`ACP_PROVIDER_TYPE`, model, credential environment reference, timeout, pricing, per-dispatch cost, and daily cost must satisfy the existing provider runbook. The `agent_step.model` must exactly equal the configured provider's default model; a mismatch fails before reservation or invocation so model-specific pricing cannot be bypassed. `ACP_SCHEDULER_MAX_RETRIES` controls retryable node failures in both normal and dynamic scheduler modes, defaults to `0` for compatibility, and fails startup unless it is in `0..=10`; it does not make outcome-unknown tool effects retryable or weaken provider cost, timeout, or circuit-breaker gates. The scheduler lease must be at least the maximum bounded executor timeout plus `max(interval, 1000 ms)`; the example uses 302 seconds for the 300-second Agent Runtime bound and a two-second interval. Do not put a raw credential in a plan, policy, audit record, command, or evidence artifact. Startup fails if a real provider-backed Agent Runtime is available without `ACP_REQUIRE_AUTH=1`. CI uses deterministic fixtures and must not set live credentials.

Create one or more confirmed, dependency-ordered typed nodes, then create and advance the run. Each node still performs exactly one action. Repeated entries for one agent must carry the same role, profile, and unique capability set so the run-scoped agent state has one stable identity. Replace the symbolic URL/key and carry the returned `plan_id` and `run_id`; do not copy raw provider content into operator records.

```bash
curl -sS -X POST "$ACP_API_URL/api/v1/plans" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{
    "raw_request":"Perform one bounded agent decision",
    "request_source":"operator",
    "agent_steps":[{
      "agent_id":"agent-operator-1",
      "role":"operator",
      "capability_profile":["memory","mailbox","child_task","handoff","review","debate"],
      "profile_id":"operator-bounded",
      "model":"configured-model"
    }],
    "confirm_agent_runtime_plan":true
  }'

curl -sS -X POST "$ACP_API_URL/api/v1/workflow-runs" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{"plan_id":"REPLACE_WITH_PLAN_ID","confirm_execution":true}'

curl -sS -X POST "$ACP_API_URL/api/v1/workflow-runs/REPLACE_WITH_RUN_ID/tick" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{"executor":"agent_step","max_retries":0}'
```

Capabilities are authoritative, not prompt hints: `memory` permits digest/note/observation updates; `mailbox` permits read/acknowledge; `child_task` permits child proposals; `handoff`, `review`, and `debate` permit only their corresponding typed actions. `cancel_proposal` requires at least one coordination capability. `wait` and `complete` are always available. Unknown or omitted capabilities grant nothing. Mailbox observations expose only bounded immutable IDs needed for later review/debate/handoff actions and never include raw bodies.

Configure tool policy through the authenticated hash-bound resources. First PUT a capability with `confirm_tool_policy=true`; then PUT a profile allowlist or hook. A first create omits `expected_current_sha256`. Before replacing an existing resource, GET it and copy its `resource.resource_sha256` into `expected_current_sha256`. A stale hash returns `409 tool_policy_stale`. Repeating an already-applied identical document returns `changed=false`. An explicit empty allowlist is deny-all.

```bash
curl -sS -X PUT "$ACP_API_URL/api/v1/tool-policy/capabilities/echo" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{
    "description":"bounded echo command",
    "requires_approval":true,
    "risk_level":"medium",
    "confirm_tool_policy":true
  }'

curl -sS -X PUT "$ACP_API_URL/api/v1/tool-policy/profiles/operator-bounded/allowlist" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{"tool_names":["echo"],"confirm_tool_policy":true}'
```

Capability, allowlist, and hook mutation plus audit commit atomically. Configuration rejects unknown tools, duplicate IDs, oversized or secret-shaped metadata, stale hashes, invalid hook actions, and more than 32 enabled hooks. Dashboard Settings provides a read-only resource/hash inspector. Approval-required execution enters `awaiting_approval`; resolve the corresponding typed operator decision with its current queue hash and `dispatch:execute`. Do not use the metadata-only workflow-approval POST as execution authority. An approved authorization is bound to exact run, node, tool, profile, action hash, and request, and is consumed before one subprocess invocation. Non-approval tools claim an atomic implicit receipt before invocation. A retry cannot repeat the effect; if execution or a post hook fails after that claim, the node records an explicit non-retryable outcome-unknown failure for operator reconciliation.

Use only a documented scheduler executor value. Startup rejects unknown `ACP_SCHEDULER_EXECUTOR` and `ACP_EXECUTION_MODE` values, and an unavailable configured CLI executor produces an explicit failed result rather than a noop. Direct `ACP_EXECUTION_MODE=cli`, `ACP_EXECUTION_MODE=auto`, and `/api/v1/dispatch` multi/CLI execution are retired. Use `ACP_SCHEDULER_EXECUTOR=auto` or `pool` for provider/CLI hybrid workflows; CLI tools then execute only as confirmed leased nodes with policy and receipt enforcement. Bind every CLI node to the exact app-owned supervised-patch workspace for its run; a missing or different path fails with no subprocess and no cwd fallback. Repeating a supervised-patch verification request reuses the canonical workspace/operation/attempt run when the exact binding matches, returns its terminal result, or reports `verification_in_progress`; a changed binding returns conflict. The run is marked `api_owned_supervised_patch`, so scheduler workers skip it in queue-enabled and queue-disabled modes. If the engine also mounts a scheduler, configure its lease timeout to exceed the larger of the verification-command timeout and CLI timeout by `max(interval, 1000 ms)`; otherwise the handler returns `unsafe_managed_execution_lease` before creating a run or subprocess effect. Treat a policy read error caused by corrupt stored JSON as an integrity incident; do not overwrite it through a guessed hash. Stop execution, back up the store, run the integrity procedure, and restore from verified app-owned evidence.

Emergency disable and rollback:

1. Set `ACP_AGENT_RUNTIME_KILL_SWITCH=1` and pause the existing scheduler.
2. Inspect active leases and `awaiting_approval` nodes; do not drop policy/receipt tables while one is active.
3. Restore a prior tool resource by PUTting the previous bounded snapshot recorded in `tool_policy.*_configured` audit details with the current hash.
4. The default removal path is to revert the integration merge after leases are safe and leave migration v22 inert so it preserves restart evidence.
5. If destructive local cleanup is required, perform it before reverting the code: after a verified backup and stopped v22 writers, invoke `LocalProductStore::rollback_v22_to_v21` with explicit destructive-local confirmation, then revert the integration merge. The transaction refuses any non-empty v22 authority table, audits a successful empty-table rollback, drops only the three v22 tables, and moves the backend version marker to v21 atomically. Do not bypass refusal by manually dropping receipt, authorization, or configured-allowlist evidence.

Post-execution hooks run after the external effect. A post block marks the node result failed, preserves inner usage fields, and is non-retryable because the effect may already have occurred; it cannot undo the external effect. Use pre-execution block/approval hooks when execution itself must be prevented.

## Durable Memory, Budget Producer, and Replay Operations

Durable memory requires exact tenant/workspace scope and `dispatch:execute` for mutation. Start with embeddings disabled or explicitly enable the local harness-derived vector mode outside CI:

```bash
export ACP_DURABLE_MEMORY_EMBEDDING_MODE=disabled
# Optional local vector mode, never in CI:
# export ACP_DURABLE_MEMORY_EMBEDDING_MODE=local_hash_v1
# export ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS=1
```

Provider embeddings are separately default-off and use the existing symbolic credential, provider audit/cost reservation, timeout/retry, circuit-breaker, and kill-switch boundaries. The production contract is pinned to OpenRouter's embedding endpoint, `nvidia/llama-nemotron-embed-vl-1b-v2:free`, and 1,536 dimensions. Every call revalidates the current OpenRouter embedding catalog, exact canonical model identity, embedding capability, and complete zero-price prompt/completion record before the embedding request. A catalog/model/pricing change fails closed and requires a reviewed contract update; it never falls back to local hashing or lexical retrieval.

```bash
# OPENROUTER_API_KEY must already exist in this process-private environment.
test -n "${OPENROUTER_API_KEY:-}"
export ACP_DURABLE_MEMORY_EMBEDDING_MODE=provider
export ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS=1
export ACP_DURABLE_MEMORY_EMBEDDING_CREDENTIAL_ENV=OPENROUTER_API_KEY
export ACP_DURABLE_MEMORY_EMBEDDING_TIMEOUT_MS=20000
export ACP_DURABLE_MEMORY_EMBEDDING_MAX_RETRIES=2
export ACP_DURABLE_MEMORY_EMBEDDING_PER_CALL_CAP_USD=0.01
export ACP_DURABLE_MEMORY_EMBEDDING_DAILY_CAP_USD=0.10
unset ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH
```

Do not paste a credential into this command or shell history; obtain it through the trusted-local process environment. `ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH=1` blocks new calls, provider mode is prohibited when `CI` is set, and `fixture` remains test-only. Provider responses persist only model/pricing/usage/vector hashes and scope/source/version bindings; raw memory text is excluded from provider audit and scorecards. Lexical retrieval occurs only when the request explicitly sets `allow_lexical_fallback=true`; its result remains labeled `lexical_fallback`.

Create and retrieve one bounded record. Replace IDs and hashes with exact app-owned values; never place raw credentials, provider transcripts, repository content, or private paths in memory:

```bash
curl -sS -X POST "$ACP_API_URL/api/v1/memories" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{
    "scope":{"tenant_id":"local","workspace_id":"REPLACE_WITH_RUN_WORKSPACE","agent_id":null,"task_id":null},
    "run_id":"REPLACE_WITH_RUN_ID",
    "source_id":"operator-source-1",
    "source_sha256":"REPLACE_WITH_64_HEX_SOURCE_HASH",
    "conflict_key":"bounded-fact-1",
    "content":{"summary":"bounded approved fact"},
    "confidence":0.9,
    "fresh_until":null,
    "expires_at":null,
    "supersedes_memory_id":null
  }'

curl -sS -X POST "$ACP_API_URL/api/v1/memories/retrieve" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{
    "scope":{"tenant_id":"local","workspace_id":"REPLACE_WITH_RUN_WORKSPACE","agent_id":null,"task_id":null},
    "run_id":"REPLACE_WITH_RUN_ID",
    "node_id":"REPLACE_WITH_NODE_ID",
    "query":"bounded approved fact",
    "top_k":5,
    "max_tokens":256,
    "max_bytes":4096,
    "allow_lexical_fallback":true
  }'
```

Revision, invalidation, forget, and supersede require the authoritative `run_id`, exact scope, and latest exact version. A conflict never silently overwrites either record. Resolve the existing two-record conflict pair before adding another incompatible fact; a third member is rejected without mutation. Forget deletes prior version content and leaves a metadata-safe tombstone; prune requires explicit confirmation and removes at most the bounded expired batch. Inspect with `GET /api/v1/memories/:memory_id?run_id=REPLACE_WITH_RUN_ID`. Scheduler injection uses the run's stored tenant/workspace and cannot be broadened by node metadata.

Terminal native scorecard persistence automatically invokes the fenced budget producer. Operators may deterministically recompute the same run through the authenticated owner:

```bash
curl -sS -X POST "$ACP_API_URL/api/v1/budget-evidence/recompute" \
  -H "authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "content-type: application/json" \
  -d '{"run_id":"REPLACE_WITH_RUN_ID","confirm_recompute":true}'
```

Inspect measurement provenance in the returned normalized observations and existing scorecard/Dashboard surfaces. Missing provider, model, pricing, billed-token, or cost fields remain unavailable; do not reinterpret them as zero. Repeated producer execution or restart reuses the deterministic fenced job and immutable artifacts. The scheduler persists an ascending run cursor and bounded rotating retry set; do not delete its app config while recovery is active. Applying an anomaly still requires a fresh supported critical finding, the existing enabled pause policy, `dispatch:execute`, and an explicit operator action. Recovery remains a separate audited action.

Replay production is provider-free and shadow-only. `PUT /api/v1/offline-replays/production-profile` requires configured authentication, `team:admin`, `confirm_profile=true`, a bounded disabled/enabled profile, exact hash-valid current/candidate policies, and a bounded dispatch window. A normal persisted dispatch then invokes the profile's bounded replay producer. Scheduler ticks also advance a persisted ascending dispatch-history cursor and bounded rotating retry set, recovering immediate-call failures and restart gaps without a second queue. `POST /api/v1/offline-replays/generate` provides confirmed deterministic recomputation with `dispatch:execute`. Neither path mutates active policy. Promotion requires the evidence-chain endpoint's exact replay artifact/binding, active and candidate policy identities, canary evidence, current-state rebinding, confirmation, and permission. Use the typed operator rollback only with the exact snapshot; inspect is read-only and acknowledge binds exact source kind/ID/hash without implying approval.

PR2 rollback procedure:

1. Set `ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH=1`, disable embedding generation and replay production, pause scheduler admission, and drain active memory/budget jobs.
2. Take a verified SQLite online backup or PostgreSQL operator backup and run integrity checks.
3. Prefer reverting the integration merge while retaining migration v25 so provider binding evidence remains inspectable. A v25-to-v24 downgrade is permitted only before any provider embedding binding exists; `rollback_v25_to_v24` locks the version/table and refuses non-empty binding columns.
4. Destructive local downgrade is permitted only while the matching migration code is installed, every writer is stopped, and the affected authority is empty. After a permitted v25 rollback, `rollback_v24_to_v23` and then `rollback_v23_to_v22` retain their existing explicit-confirmation, atomic audit, and non-empty-authority refusal. Never manually drop embedding bindings, memory, usage, job, replay-binding, or acknowledgement evidence.

## Bounded PE-6 Recovery Drills

The drill CLI accepts only registered scenario IDs or named suites. It uses
temporary resources and fixed owner-test commands; it does not call providers,
publish releases, modify host installations, or target a real database.

Run the supported local suite and save its bounded report under `/tmp`:

```bash
PYTHONPATH=. uv run --no-project python tools/run_fault_drills.py \
  --suite core --seed 0 --worker 0 --output /tmp/acp-pe6-core.json
```

Run storage drills. Without the GitHub Actions PostgreSQL service, the
PostgreSQL entry is reported as `unsupported`; it is not counted as a pass.
The existing `pg-integration-tests` job supplies the exact disposable
`ACP_TEST_DATABASE_URL` and service identity for that owner path; arbitrary
local database URLs remain unsupported.

```bash
PYTHONPATH=. uv run --no-project python tools/run_fault_drills.py \
  --suite storage --seed 0 --worker 0 --output /tmp/acp-pe6-storage.json
```

Inspect a human summary or the canonical JSON directly:

```bash
PYTHONPATH=. uv run --no-project python tools/run_fault_drills.py \
  --scenario-id pe6.release.provenance_rollback.v2 --format human
PYTHONPATH=. uv run --no-project python tools/run_fault_drills.py \
  --scenario-id pe6.release.provenance_rollback.v2 --format json
```

Use `--require-supported` only when the named environment capability is a
prerequisite for the operator's run. A failed, aborted, invalid, or cleanup
failed result always returns non-zero. Reports use the v2 owner-evidence
boundary and bind the source head, scenario/version/hash, fault and injection
point, owner/resources, scenario-specific checks, exact owner-evidence hash,
registry, seed, worker, configured timeout, monotonic observed duration, and
independent cleanup. Exit zero without valid owner evidence fails. A category
without a scenario-specific owner check remains `unsupported`, never passed.

## Local Token-Efficiency Runner

Run the #154 local stateful-vs-stateless comparison:

```bash
uv run --no-project python scripts/provider_gated_real_runner.py --output-dir /tmp/acp-local-runner --iterations 10
```

Expected files:

```text
/tmp/acp-local-runner/stateless_reread.scorecard.json
/tmp/acp-local-runner/stateful_store.scorecard.json
/tmp/acp-local-runner/comparison.json
```

Print only the comparison:

```bash
uv run --no-project python scripts/provider_gated_real_runner.py --compare --iterations 10
```

Run the Rust local runner with the gated OpenAI-compatible live provider:

```bash
export ACP_ENABLE_PROVIDER_EXECUTION=1
export ACP_LOCAL_RUNNER_PROVIDER_TYPE=openai_compatible
export ACP_LOCAL_RUNNER_BASE_URL=https://api.openai.com/v1
export ACP_LOCAL_RUNNER_MODEL=gpt-4o-mini
export ACP_LOCAL_RUNNER_API_KEY_ENV=OPENAI_API_KEY
export ACP_PROVIDER_INPUT_COST_PER_1K_USD="REPLACE_WITH_PROVIDER_INPUT_RATE"
export ACP_PROVIDER_OUTPUT_COST_PER_1K_USD="REPLACE_WITH_PROVIDER_OUTPUT_RATE"
cargo run -p engine --bin local-runner-exec -- \
  --provider live \
  --iterations 10 \
  --db .agent-control-plane/local-team.db \
  --output-dir /tmp/acp-local-runner-live
```

`ACP_LOCAL_RUNNER_API_KEY_ENV` is a symbolic environment-variable reference, not the secret value; the referenced variable must already be present in the operator environment. Pricing must be the provider's positive USD cost per 1,000 tokens. Do not put provider credentials in command lines, docs, scorecards, logs, or artifacts. Live mode writes bounded redacted request/response/error and cost evidence to the app-owned database, reserves worst-case call cost before invocation, and fails closed unless gates, metadata, credential reference, pricing, budget, timeout, and audit storage are ready. Set `ACP_LOCAL_RUNNER_KILL_SWITCH=1` to block new live calls immediately.

Focused validation:

```bash
python -m py_compile scripts/provider_gated_real_runner.py tools/test_provider_gated_real_runner.py
uv run --no-project python -m unittest tools.test_provider_gated_real_runner
```

Validate the local runner and export app-owned scorecard artifacts:

```bash
uv run --no-project python scripts/validate_local_runner.py \
  --output-dir /tmp/acp-local-runner \
  --artifact-dir /tmp/acp-local-runner-artifacts \
  --iterations 10 \
  --keep-output
```

Expected artifact files:

```text
/tmp/acp-local-runner-artifacts/stateless_reread.artifact.json
/tmp/acp-local-runner-artifacts/stateful_store.artifact.json
```

Import exported `native_scorecard_artifact.v1` files into the local app-owned store:

```bash
cargo run -p engine --bin import-native-scorecard-artifacts -- \
  --db "${ACP_DB_PATH:-.agent-control-plane/local-team.db}" \
  /tmp/acp-local-runner-artifacts
```

The import is idempotent. It records through `LocalProductStore`, keeps artifacts read-only and metadata-only, and rejects raw prompt/output/transcript-shaped fields or secret-shaped values.

Check scorecard API reads after the engine is running:

```bash
curl -fsS "http://127.0.0.1:8080/api/v1/scorecards?run_id=real-runner-stateful_store"
curl -fsS "http://127.0.0.1:8080/api/v1/operator/evidence/real-runner-stateful_store"
```

If `ACP_REQUIRE_AUTH=1`, include the local admin bearer token:

```bash
curl -fsS -H "Authorization: Bearer ${ACP_ADMIN_API_KEY}" \
  "http://127.0.0.1:8080/api/v1/scorecards?run_id=real-runner-stateful_store"
curl -fsS -H "Authorization: Bearer ${ACP_ADMIN_API_KEY}" \
  "http://127.0.0.1:8080/api/v1/operator/evidence/real-runner-stateful_store"
```

Acceptance:

- both scorecards validate as `token_efficiency_scorecard.v1`;
- both modes use the same scenario id;
- `stateless_reread` is the baseline row;
- `stateful_store` has a positive token-reduction ratio on the deterministic task;
- the runner emits bounded comparison files and does not mutate runtime storage.
- exported artifacts import into `LocalProductStore` as read-only `native_scorecard_artifact.v1` rows;
- `GET /api/v1/scorecards?run_id=...` and `GET /api/v1/scorecards/{artifact_id}` expose app-owned artifacts;
- `GET /api/v1/operator/evidence/:run_id` exposes bounded scorecard metadata and derived metrics, not `steps`, raw traces, prompts, outputs, transcripts, or unbounded content.

## Managed LangGraph External Runtime

Install the separate locked adapter package and point the Rust engine at its stable file entrypoint. The scheduler remains the only lease/run owner; do not run the adapter as a daemon or queue worker.

```bash
uv sync --frozen --project adapters/langgraph
export ACP_ENABLE_LANGGRAPH_RUNTIME=1
export ACP_LANGGRAPH_MODE=fixture
export ACP_LANGGRAPH_PYTHON="$PWD/adapters/langgraph/.venv/bin/python"
export ACP_LANGGRAPH_ADAPTER_PATH="$PWD/adapters/langgraph/runner.py"
export ACP_LANGGRAPH_TIMEOUT_MS=30000
export ACP_LANGGRAPH_TOKEN_CAP=20000
export ACP_LANGGRAPH_PER_CALL_COST_CAP_USD=0.01
export ACP_LANGGRAPH_RUN_COST_CAP_USD=0.05
export ACP_LANGGRAPH_DAILY_COST_CAP_USD=0.10
```

Create a workflow node with task and executor `langgraph_external`, an `external_runtime_node.v1` metadata object, one of the four exact memory strategies, a bounded thread ID, and a hash-bound benchmark. Each lease performs one invocation. Inspect metadata only with `GET /api/v1/workflow-runs/{run}/nodes/{node}/external-runtime-checkpoint?thread_id=...`; the endpoint requires `dispatch:read` and exact tenant scope.

Live mode additionally requires the existing authenticated provider configuration, symbolic credential, positive pricing, provider gates, and exact confirmation:

```bash
export ACP_LANGGRAPH_MODE=live
export ACP_LANGGRAPH_LIVE_CONFIRM=I_UNDERSTAND_THIS_CALLS_A_PAID_PROVIDER
```

Never set live mode in CI. Set `ACP_LANGGRAPH_KILL_SWITCH=1` to refuse new work and terminate an active adapter child. A `provider_outcome_unknown` result requires operator inspection and must not be retried automatically.

Build and run the deterministic canonical benchmark without provider calls:

```bash
cargo build -p engine --bin efficiency_native_runtime --bin efficiency_langgraph_runtime
uv run --no-project python scripts/efficiency_live_benchmark.py \
  --mode fixture \
  --native-cli "$PWD/target/debug/efficiency_native_runtime" \
  --langgraph-adapter "$PWD/target/debug/efficiency_langgraph_runtime" \
  --output-root /tmp/acp-efficiency-fixture \
  --benchmark-run-id fixture-acceptance
```

For a live run, use `--mode live`, exact `--live-confirmation I_CONFIRM_BOUNDED_LIVE_PROVIDER_COSTS`, `--provider openai_compatible`, an HTTPS base URL, fixed model/tokenizer, `--credential-env`, `--kill-switch-env`, explicit pricing identity/effective date/rates, low per-call/run/daily caps, an existing audit-store parent, and a private output root. The command refuses CI, incomplete provider token usage, missing audit evidence, missing provider calls, incomparable identities, and quality regression. Each runtime persists its four memory and two tool-discovery scorecards into that app-owned store; inspect the two scenario matrices through the existing scorecard API/Dashboard. It never writes a credential, raw prompt, raw provider response, transcript, or repository content to the report.

Rollback: set both kill switches, stop scheduler admission, inspect active leases/blocked receipts, restore a verified database backup if destructive v24 removal is required, and revert the implementation. Leaving v24 rows inert is preferred.

## Bounded LangGraph Scorecard Import

The legacy importer accepts summary-level JSON only and remains a compatibility/operator path. It does not itself run a graph or call a provider. Prepare stateless and stateful summaries outside the product runtime with the same comparison contract: scenario/task digests, runtime/version, provider/model, tokenizer, pricing, quality method/threshold, evaluator version, redaction/retry policy, and seed.

The checked pilot can be recaptured explicitly with the development-only tool below. This transient command does not add LangGraph to the engine/app dependency graph, call a model/provider, or persist graph state; it writes only the two bounded summaries. The tool's SHA-256 is bound in each fixture's `evidence_provenance.source_capture_sha256`:

```bash
uv run --no-project \
  --with langgraph==1.2.9 \
  --with tiktoken==0.12.0 \
  python tools/capture_langgraph_pilot.py \
  --output-dir /tmp/acp-langgraph-pilot-evidence
```

Generate runtime-neutral v2 artifacts without relabeling LangGraph as native:

```bash
uv run --no-project python scripts/langgraph_trace_import.py \
  /tmp/langgraph-stateless-summary.json \
  --artifact \
  --output /tmp/langgraph-stateless.artifact.json

uv run --no-project python scripts/langgraph_trace_import.py \
  /tmp/langgraph-stateful-summary.json \
  --artifact \
  --output /tmp/langgraph-stateful.artifact.json
```

Compare the summaries before persistence:

```bash
uv run --no-project python scripts/langgraph_trace_import.py \
  /tmp/langgraph-stateless-summary.json \
  /tmp/langgraph-stateful-summary.json \
  --compare \
  --output /tmp/langgraph-comparison.json
```

Import through the existing app-owned importer and table. The binary name is retained for backward compatibility; it accepts both `native_scorecard_artifact.v1` and `scorecard_artifact.v2`:

```bash
cargo run -p engine --bin import-native-scorecard-artifacts -- \
  --db "${ACP_DB_PATH:-.agent-control-plane/local-team.db}" \
  /tmp/langgraph-stateless.artifact.json \
  /tmp/langgraph-stateful.artifact.json
```

Repeating the command must report both files as `unchanged`. After the engine starts, read the scenario comparison:

```bash
curl -fsS \
  "http://127.0.0.1:8080/api/v1/scorecards?scenario_id=langgraph_offline_state_retention_pilot_2026_07_10"
```

The Dashboard exposes the same bounded view under `#benchmarks`.

Run the focused importer tests:

```bash
uv run --no-project python -m unittest tools.test_langgraph_trace_import
cargo test -p engine --test test_native_scorecard_artifacts \
  fixed_langgraph_pilot_import_is_idempotent_and_visible_end_to_end
```

Acceptance:

- both inputs pass bounded-field, raw-trace, and secret-shaped-value rejection;
- new imports are capped at 1 MiB, 1 KiB per JSON string/key, 1,000 items per array (including steps), 128 fields per object, and 16 nested levels;
- modes are exactly `stateless_reread` and `stateful_store` with one shared `scenario_id`;
- both runs have identical comparison contracts and explicit baseline/candidate roles;
- the comparison reports tokens, repeated-context ratio, cost, latency, retries, and quality;
- no raw LangGraph state, checkpoint, message, span, prompt, output, tool payload, repository content, private path, or credential is retained;
- token/cost advantage is reported only when both runs meet the shared quality threshold;
- canonical content hash and derived metrics are recomputed before persistence;
- the fixed offline LangGraph 1.2.9 pilot reproduces 38,452 baseline tokens, 11,294 candidate tokens, and 70.6283% token reduction; both quality scores are 1.0 and both costs are $0, so no cost advantage is reported.

Rollback requires no schema down migration. Revert the implementation commit; v2 rows remain JSON-readable in the existing table and old run/id API paths. If operators also want to remove pilot rows, restore a verified pre-import database backup instead of running an automatic destructive cleanup.

## Local Engine Reminder

For normal local engine operation, use the existing dashboard build, engine start, health check, metrics, backup, restore, release, and incident-triage scripts in `scripts/` and the CI workflow as the source of truth.

## Event-Driven Agent Orchestrator Push Credential

The implementation and CI-repair finalizers require the repository secret `AGENT_PUSH_TOKEN`: a fine-grained repository PAT with **Contents: Read and write** only. It is used only by the GitHub-hosted finalizer's `Push branch with isolated temporary credentials` step. The worker preflight separately requires `AGENT_SETTINGS_READ_TOKEN` with repository **Administration: read** only so it can verify that Actions is allowed to create pull requests before consuming Vader capacity. Neither token is copied to Vader. PR creation/update, labels, comments, dispatch, control reads, review, and merge otherwise use the workflow `${{ github.token }}` with explicit least-privilege permissions.

The PAT is never copied to Vader, artifacts, or remote URLs. The push step fails closed if it is missing and uses a temporary `GIT_ASKPASS` directory below `RUNNER_TEMP`; cleanup removes only that directory. It never calls `gh auth setup-git` and never changes the runner user's global Git credential helper. Rotate or revoke the PAT immediately after any suspected exposure.

Create the disabled control surface once with `python3 scripts/agent-control/control_state.py setup-controls --repo OWNER/REPO` (the `setup` spelling remains an explicit compatibility alias; `setup_labels.py` delegates to the same owner). This is the single complete, idempotent setup command: it paginates and verifies all fifteen required operational/control labels, preserves unrelated labels and metadata, and ensures exactly one open Issue titled `[agent-control] Orchestrator controls` with marker `<!-- agent-orchestrator-control:v1 -->`. A newly created or repaired control Issue has `agent-control` and `agent-emergency-stop`, but neither enable label. Setup never enables orchestration, removes the stop, or silently accepts an ambiguous/malformed Issue. Operators change only that Issue's labels:

```bash
uv run --no-project python scripts/agent-control/control_state.py status --repo OWNER/REPO
uv run --no-project python scripts/agent-control/control_state.py enable-orchestrator --repo OWNER/REPO
uv run --no-project python scripts/agent-control/control_state.py disable-orchestrator --repo OWNER/REPO
uv run --no-project python scripts/agent-control/control_state.py enable-auto-merge --repo OWNER/REPO
uv run --no-project python scripts/agent-control/control_state.py disable-auto-merge --repo OWNER/REPO
uv run --no-project python scripts/agent-control/control_state.py emergency-stop --repo OWNER/REPO
uv run --no-project python scripts/agent-control/control_state.py emergency-resume --repo OWNER/REPO
```

Emergency stop always wins. It atomically (or with verified compensation) adds `agent-emergency-stop` and removes both enable labels. Resume removes only the stop; it never restores prior authorization. Re-enable is explicit: `enable-orchestrator` requires one valid open control Issue and no stop, while `enable-auto-merge` additionally requires the live orchestrator label and never enables orchestration itself. Every command rereads and verifies live state after mutation; a provider success with a mismatched resulting state is failure-closed. A workflow that was already active may perform only its idempotent failure cleanup: the state owner removes that workflow's one active-capacity label and records a non-running blocked label, with exact-head validation for review and repair. This cleanup cannot dispatch or authorize work. Keep orchestration and auto-merge disabled, with emergency stop present, until exact-head CI, Issue↔PR binding, independent review, and Vader service-user validation have all been independently revalidated.

Canonical CI acquisition binds repository, trusted head repository, workflow identity/path, branch, exact head SHA, and PR. Completed supported evidence outranks active runs; the newest authoritative completed result wins, with a natural `pull_request` run breaking otherwise equal ties. Unsupported terminal runs are reselected around, and a pending natural run receives at most one bounded `workflow_dispatch` fallback. Observed, selected, superseded, unsupported, and fallback state is persisted so stale or duplicate events cannot dispatch duplicate repairs/reviews.

The Codex wrapper constructs an allowlisted child environment for version, login-status, help, implementation, repair, and review calls. It preserves only the documented runtime/login variables (`HOME`, optional `CODEX_HOME`, `PATH`, locale/temp/terminal and service-user identity variables) and excludes GitHub, provider, cloud, and unknown secret-shaped variables. It does not fall back to API-key billing, mutate login files, print the environment, or retain raw failure output.

### Self-hosted runner readiness

Use the repository-owned bounded readiness checker for the Actions runner. It verifies the local runner files by metadata only, checks the listener version, resolves exactly one systemd service layout, and uses fully paginated GitHub runner status. The default check requires the runner to be online and idle; `--allow-busy` is only for a check deliberately executed from inside the active runner job.

```bash
export AGENT_REPO=OWNER/REPO
export AGENT_RUNNER_ROOT=/path/to/actions-runner
export AGENT_RUNNER_NAME=Vader
uv run --no-project python scripts/agent-control/runner_readiness.py \
  --repo "$AGENT_REPO" \
  --runner-root "$AGENT_RUNNER_ROOT" \
  --runner-name "$AGENT_RUNNER_NAME"

AGENT_REPO="$AGENT_REPO" \
AGENT_RUNNER_ROOT="$AGENT_RUNNER_ROOT" \
AGENT_RUNNER_NAME="$AGENT_RUNNER_NAME" \
bash scripts/agent-control/preflight.sh --verbose --require-runner
```

The official GitHub Actions runner `config.sh` is an installation/registration command, not a health-check interface. `config.sh --check` is not a supported readiness check and must not be used; use `runner_readiness.py` instead. The checker never reads or prints `.credentials` or `.credentials_rsaparams` contents.

Every task Issue intended for implementation must also declare its permitted change scope. The finalizer rejects an artifact unless every changed path is exact or under an allowed directory prefix:

```html
<!-- agent-orchestrator-scope:v1 {"allowed_paths":["scripts/agent-control/","tests/test_agent_orchestrator_artifacts.py"]} -->
```

Before commit, the finalizer performs bounded structural validation only: it validates the artifact schema, hashes and exact bindings, rechecks Issue scope, recomputes the staged path set, and runs `git diff --cached --check`. It does not claim arbitrary task-specific behavioral validation at that point. Behavioral acceptance comes from the canonical exact-head seven-job CI run acquired after the validated commit is pushed.

Review terminal states are explicit. The validator accepts only schema-valid exact-head artifacts. Exact `PASS` additionally requires the complete bounded diff, no blockers, and affirmative exact-head CI, security, and rollback gates; it removes `review-running`, adds `review-passed` and `agent-merge-ready`, and waits without consuming capacity when auto-merge is disabled. Schema-valid `PASS_WITH_NOTES`, `BLOCKED`, and `FAIL` are normal non-authorizing business outcomes: their bounded verdict, summary, notes, blockers, digest, and reviewed head are recorded, active review capacity is released, and `agent-review-blocked` is set. Malformed, unavailable, oversized, or head-mismatched output exits nonzero, is never recorded as a verdict, and records only a bounded malformed/infrastructure reason before the matching failure cleanup. Merge additionally requires GitHub's current review decision and latest effective human review per reviewer (not a raw historical `CHANGES_REQUESTED` scan), plus complete cursor-paginated review-thread evidence with no unresolved thread; unavailable, contradictory, partial, malformed, or bounded-out review data fails closed. After operator inspection, use the live-gated controller command `retry-review`; it derives the current PR and exact head from trusted state, revalidates their binding, and dispatches a fresh read-only review without returning the Issue to implementation-ready state. Stage B remains incomplete; keep the orchestrator disabled and emergency-stopped.

## Release Upgrade and Rollback

Never execute a mutable branch script or an unverified pipe. Download the exact
tagged bootstrap asset and its SLSA bundle, verify the local bytes against the
exact repository, release workflow, tag ref, source commit, GitHub OIDC issuer,
and predicate type, then execute that immutable local file. Replace the example
tag and 40-character commit with the release's published immutable identity:

```bash
VERSION=v0.1.0
SOURCE_COMMIT=0123456789abcdef0123456789abcdef01234567
BASE="https://github.com/Igzela/token-efficient-agent-harness-lab/releases/download/${VERSION}"
curl --fail --location --output install-from-release.sh "${BASE}/install-from-release.sh"
curl --fail --location --output install-from-release.sh.slsa.bundle.json \
  "${BASE}/install-from-release.sh.slsa.bundle.json"
gh attestation verify install-from-release.sh \
  --bundle install-from-release.sh.slsa.bundle.json \
  --predicate-type https://slsa.dev/provenance/v1 \
  --repo Igzela/token-efficient-agent-harness-lab \
  --signer-workflow Igzela/token-efficient-agent-harness-lab/.github/workflows/release.yml \
  --source-ref "refs/tags/${VERSION}" --source-digest "${SOURCE_COMMIT}" \
  --cert-oidc-issuer https://token.actions.githubusercontent.com \
  --deny-self-hosted-runners
bash ./install-from-release.sh --version "${VERSION}" \
  --source-commit "${SOURCE_COMMIT}" \
  --bootstrap-bundle ./install-from-release.sh.slsa.bundle.json
```

The bootstrap re-verifies itself before any download, verifies the separately
attested verifier asset, downloads the archive and exact local SLSA, SPDX, and
`release_provenance.v2` manifest bundles, compares signed predicates with the
canonical local SBOM/manifest, then enforces archive bounds before extraction.
API-fetched attestations are not installation evidence.

From an already verified and extracted release directory, upgrade a user-local
installation atomically. All six evidence paths must name the exact archive,
canonical documents, and distributed bundles:

```bash
./upgrade.sh \
  --prefix "$HOME/.local" \
  --data-dir "$HOME/.agent-control-plane" \
  --artifact "$ARCHIVE" \
  --sbom "$ARCHIVE.spdx.json" \
  --manifest "$ARCHIVE.release-manifest.json" \
  --slsa-bundle "$ARCHIVE.slsa.bundle.json" \
  --spdx-bundle "$ARCHIVE.spdx.bundle.json" \
  --manifest-bundle "$ARCHIVE.release-manifest.bundle.json"
```

The upgrade verifies release evidence before stopping or mutating anything,
stages binary and Dashboard, retains verified backups, and requires restart and
health checks. It prints `UPGRADE_FAILED_ROLLBACK_SUCCEEDED` only after the old
binary digest, Dashboard state when present, previous process/restart hook, and
health all verify. Otherwise it prints `UPGRADE_FAILED_ROLLBACK_FAILED`, exits
with a distinct status, and preserves backup evidence. After repairing the
operator environment, repeated `./upgrade.sh ... --recover` is idempotent.

For a source checkout or non-publishing local package dry run only, use the explicit development mode; it does not claim production provenance:

```bash
./install.sh --prefix "$HOME/.local" --development
./upgrade.sh --prefix "$HOME/.local" --data-dir "$HOME/.agent-control-plane" --development
```

For a managed service, provide paired explicit hooks:

```bash
./upgrade.sh \
  --prefix "$HOME/.local" \
  --data-dir "$HOME/.agent-control-plane" \
  --stop-command '<process-manager stop command>' \
  --restart-command '<process-manager start command>'
```

Both hooks are required together. A managed service should also supply
`--health-command` so recovery proves the restored process, not merely the
binary's help output. Validate the packaged upgrade contract with
`bash scripts/check_release_contract.sh` before publishing a release.
