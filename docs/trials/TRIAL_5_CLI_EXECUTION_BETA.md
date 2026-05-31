# Trial 5 — CLI Execution Beta

Date: 2026-05-31

Status: `TRIAL_5_CLI_EXECUTION_BETA_PASS_AFTER_FIXES`

## Scope

Trial 5 validated the explicit opt-in CLI execution path for local Codex and Claude Code adapters. The pilot used deterministic stub CLI binaries for execution and only discovered real local `codex`/`claude` binary paths without invoking them.

Boundaries held:

- CLI execution remains default-off.
- Provider/model API calls remain default-off.
- No target repository writes.
- No background workers.
- No cloud/SaaS deployment.
- No R8, Type Unification, `checkpoint.rs` split, `dispatch_decision.rs` split, or `app_layer/` reorganization.

## Pilot Evidence

Command run:

```bash
cargo build -p engine
uv run --no-project python scripts/trial5_cli_execution_beta.py
```

Result: passed.

Key evidence from the passing run:

- `ACP_ENABLE_CLI_EXECUTION` unset produced `executor_type: "noop"` and provider health `status: "noop"`.
- `ACP_ENABLE_CLI_EXECUTION=1` with no discoverable binaries produced noop dispatch behavior plus clear `codex binary not found` and `claude binary not found` diagnostics.
- Stub Codex dispatch routed to `codex_cli`, completed, returned output `codex ok`, token usage, estimated cost, `final_status: "completed"`, and a CLI execution gate.
- Stub Claude dispatch routed to `claude_code_cli`, completed, returned output `claude ok`, token usage, estimated cost, `final_status: "completed"`, and a CLI execution gate.
- Dispatch list/detail/search/pagination, audit, cost summary/detail, dashboard JSON, TypeScript SDK, and Python SDK all exposed CLI execution fields without provider audit events.
- Failure paths passed: missing binary, nonzero exit, malformed output, timeout, and disabled env.
- Real local discovery found `/home/igzela/.npm-global/bin/codex` and `/home/igzela/.local/bin/claude`; real binaries were not invoked.

## Flow Results

| Flow | Expected Behavior | Actual Behavior | Severity | Fix Status | Reference |
|---|---|---|---|---|---|
| Default disabled env | `ACP_ENABLE_CLI_EXECUTION` unset must never execute CLI, even when binaries exist on PATH. | Passed; dispatch used noop and provider health stayed noop. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Missing CLI binaries | Opt-in with no binaries should degrade clearly without provider calls. | Passed; dispatch used noop and startup output named missing Codex/Claude binaries. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Stub Codex success | Code-generation dispatch routes to `codex_cli` and records CLI result fields. | Passed; output, tokens, estimated cost, final status, and CLI gate were recorded. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Stub Claude success | Architecture-plan dispatch routes to `claude_code_cli` and records CLI result fields. | Passed; output, tokens, estimated cost, final status, and CLI gate were recorded. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Dispatch visibility | List/detail/search/pagination should show CLI dispatches accurately. | Passed using raw request search and detail bundle checks. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Audit/provider audit | Local dispatch audit should exist; provider audit should remain empty. | Passed; audit contained `dispatch.record`, provider audit events were empty. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Cost/token visibility | CLI token/cost fields should be recorded when available and safe when unavailable. | Passed; success rows had tokens/cost, failure rows had zero-safe cost details. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| SDK visibility | TypeScript and Python SDKs should expose CLI execution fields. | Passed; both SDK smokes read `executor_type`, `status`, token fields, and provider health noop. | polish | fixed | `scripts/trial5_cli_execution_beta.py`; `sdk/python/tests/test_client.py` |
| Failure: missing binary | Explicit missing binary should fail as CLI-not-found, not silently succeed. | Passed; `error_domain: "cli_not_found"` and `final_status: "failed"`. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Failure: nonzero exit | Nonzero CLI exit should fail with CLI execution error. | Passed; `error_domain: "cli_execution_error"` and `final_status: "failed"`. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Failure: malformed output | Successful CLI process with malformed JSON should fail because CLI output format is contractual. | Initial pilot showed malformed stdout would be treated as `cli_completed`. Fixed to `cli_output_parse_error`. | major | fixed | `engine/src/cli/codex.rs`; `engine/src/cli/claude_code.rs` |
| Failure: timeout | Slow CLI process should fail with timeout and bounded latency. | Passed; `error_domain: "cli_timeout"` and `final_status: "failed"`. | polish | fixed | `scripts/trial5_cli_execution_beta.py` |
| Dashboard wording | Dashboard should not imply provider execution for CLI cost estimates. | Initial UI copy labeled estimated cost as `provider`. Fixed copy to `executor`. | minor | fixed | `dashboard/src/components/Costs.tsx` |

## Findings

### Finding 1 — Malformed CLI Output Was Treated As Success

- Flow tested: malformed output failure path.
- Expected behavior: a CLI process that exits 0 but emits non-JSON output should fail with a parse-specific error because Codex/Claude CLI adapters request structured JSON.
- Actual behavior: before the fix, malformed stdout was accepted as raw output with `status: "cli_completed"`.
- Severity: major
- Fix status: fixed
- Fix: Codex and Claude Code CLI parsers now return `status: "failed"`, `error_domain: "cli_output_parse_error"`, `finish_reason: "parse_error"`, no cost estimate, and preserved raw stdout as output.
- Test reference: `malformed_codex_json_is_a_failed_execution`; `malformed_claude_json_is_a_failed_execution`; `scripts/trial5_cli_execution_beta.py`

### Finding 2 — Dashboard Cost Label Implied Provider Execution

- Flow tested: dashboard visibility.
- Expected behavior: local UI should describe CLI/provider/noop cost estimates generically and not imply provider execution.
- Actual behavior: the costs panel labeled estimated cost as `provider`, even though CLI estimates can contribute to the same summary.
- Severity: minor
- Fix status: fixed
- Fix: dashboard costs panel label changed from `provider` to `executor`.
- Reference: `dashboard/src/components/Costs.tsx`

### Finding 3 — Python SDK Lacked An Explicit CLI Field Regression

- Flow tested: SDK visibility.
- Expected behavior: SDK regression coverage should preserve CLI execution result fields returned by the API.
- Actual behavior: Python SDK tests covered endpoint calls but did not explicitly assert CLI execution fields.
- Severity: polish
- Fix status: fixed
- Fix: added a Python SDK regression asserting `executor_type`, `status`, token fields, and `usage_source` survive dispatch parsing.
- Test reference: `sdk/python/tests/test_client.py`

## Deferred Issues

None.

## Verification Status

Completed during pilot:

```bash
cargo build -p engine
uv run --no-project python scripts/trial5_cli_execution_beta.py
```

Full closeout verification is tracked in the final Trial 5 closeout response.
