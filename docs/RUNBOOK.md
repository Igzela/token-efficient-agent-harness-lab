# Agent Control Plane — Runbook

Operator procedures for the local Agent Control Plane.

Last updated: 2026-07-13.

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

## Bounded LangGraph Scorecard Import

The LangGraph adapter accepts summary-level JSON only. It does not import LangGraph, run a graph, call a provider, or read a repository. Prepare stateless and stateful summaries outside the product runtime with the same comparison contract: scenario/task digests, runtime/version, provider/model, tokenizer, pricing, quality method/threshold, evaluator version, redaction/retry policy, and seed.

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

The implementation and CI-repair finalizers require the repository secret `AGENT_PUSH_TOKEN`: a fine-grained repository PAT with **Contents: Read and write** only. It is used only by the GitHub-hosted finalizer's `Push branch with isolated temporary credentials` step. PR creation/update, labels, comments, dispatch, control reads, review, and merge use the workflow `${{ github.token }}` with explicit least-privilege permissions.

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

Every task Issue intended for implementation must also declare its permitted change scope. The finalizer rejects an artifact unless every changed path is exact or under an allowed directory prefix:

```html
<!-- agent-orchestrator-scope:v1 {"allowed_paths":["scripts/agent-control/","tests/test_agent_orchestrator_artifacts.py"]} -->
```

Before commit, the finalizer performs bounded structural validation only: it validates the artifact schema, hashes and exact bindings, rechecks Issue scope, recomputes the staged path set, and runs `git diff --cached --check`. It does not claim arbitrary task-specific behavioral validation at that point. Behavioral acceptance comes from the canonical exact-head seven-job CI run acquired after the validated commit is pushed.

Review terminal states are explicit: exact `PASS` requires the complete bounded diff, no blockers, and affirmative exact-head CI, security, and rollback gates; it removes `review-running`, adds `review-passed` and `agent-merge-ready`, and waits without consuming capacity when auto-merge is disabled. `PASS_WITH_NOTES`, `BLOCKED`, `FAIL`, malformed output, or an unavailable/oversized complete diff add `agent-review-blocked` and do not authorize merge. After operator inspection, use the live-gated controller command `retry-review`; it derives the current PR and exact head from trusted state, revalidates their binding, and dispatches a fresh read-only review without returning the Issue to implementation-ready state.

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
