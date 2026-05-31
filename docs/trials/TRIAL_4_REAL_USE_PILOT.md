# Trial 4 — Real-Use Pilot

Date: 2026-05-31

Status: `TRIAL_4_REAL_USE_PILOT_PASS_AFTER_FIXES`

## Scope

Trial 4 used the current Rust engine, static dashboard export, TypeScript SDK, and Python SDK as a local user would. The pilot intentionally avoided broad refactoring and stayed inside the local-only boundaries:

- no provider execution enabled by default
- no real provider/model API calls
- no target repository writes
- no background workers
- no cloud/SaaS deployment
- no R-series continuation, Type Unification, workspace split, `checkpoint.rs` split, `dispatch_decision.rs` split, or `app_layer/` reorganization

## Pilot Evidence

Command run:

```bash
cargo build -p engine
uv run --no-project python scripts/trial4_real_use_pilot.py --output artifacts/trial4/pilot-results.json
```

Result: passed.

Key evidence from the passing run:

- static dashboard root returned `Agent Control Plane`
- API key create/list/revoke passed
- default noop dispatches returned distinct dispatch IDs: `disp-0001`, `disp-0002`, `disp-0003`
- dispatch list/detail/search/pagination passed
- audit log contained the API-key revoke audit event
- backup create/list/restore/delete passed
- export/import passed
- provider health default-off returned `status: "noop"` and `message: "no provider configured"`
- TypeScript SDK basic runtime calls passed through Node against local built SDK output
- Python SDK basic runtime calls passed
- local `codex` and `claude` binaries were detected; CLI routing smoke ran with `ACP_ENABLE_CLI_EXECUTION=1` and explicit stub CLI binary paths to avoid real provider/model calls; routed executor was `codex_cli`

## Flow Results

| Flow | Expected Behavior | Actual Behavior | Severity | Fix Status | Reference |
|---|---|---|---|---|---|
| 1. Start local engine | Engine starts on loopback with temp SQLite state. | Passed with authenticated local engine and temp DB/backup dirs. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 2. Open static dashboard | Static dashboard root loads from `dashboard/out`. | Passed; root HTML contained `Agent Control Plane`. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 3. Create/list/revoke API keys | Admin key can create a scoped key, list metadata, revoke it, and see revoked metadata. | Passed. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 4. Run default noop dispatches | Default local dispatches do not call providers and produce usable dispatch records. | Initial pilot exposed duplicate `disp-0001` IDs across multiple dispatches. After fix, dispatches returned `disp-0001`, `disp-0002`, `disp-0003`. | major | fixed | `engine/src/dispatch_engine.rs` inline regression test |
| 5. Test dispatch list/detail/search/pagination | List pages, search, and detail lookup should identify the intended dispatch. | Initial pilot showed duplicate IDs made detail lookup ambiguous. After fix, list/detail/search/pagination passed. | major | fixed | `engine/src/dispatch_engine.rs`; `scripts/trial4_real_use_pilot.py` |
| 6. Test audit log | Audit endpoint returns recent local admin/user events. | Passed; audit contained `team.key.revoked`. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 7. Test backup create/list/restore/delete | Authenticated admin can create, list, restore, and delete confirmed local backups. | Passed. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 8. Test export/import | Export returns local snapshot and confirmed import is idempotent. | Passed. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 9. Test provider health default-off behavior | No provider configured should be explicit and non-error for local users. | Passed; response returned `status: "noop"` and `message: "no provider configured"`. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 10. Test TypeScript SDK basic calls | SDK can call health, dispatch list, audit, and provider health. | Passed using local Node against built SDK output. Bun-gated build/tests still require separate verification. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 11. Test Python SDK basic calls | SDK can call health, dispatch list, audit, and provider health. | Passed. | polish | fixed | `scripts/trial4_real_use_pilot.py` |
| 12. CLI routing smoke | If `codex` or `claude` exists, explicitly enable CLI routing and run a small smoke without requiring real provider calls. | Passed. `codex` and `claude` were detected; smoke used explicit stub binary paths with `ACP_ENABLE_CLI_EXECUTION=1`, and routed through `codex_cli`. | polish | fixed | `scripts/trial4_real_use_pilot.py` |

## Findings

### Finding 1 — Duplicate Dispatch IDs In One Engine Session

- Flow tested: default noop dispatches; dispatch list/detail/search/pagination
- Expected behavior: each local user dispatch has a distinct dispatch ID so list rows and detail links are unambiguous.
- Actual behavior: before the fix, multiple dispatches returned `disp-0001` because the deterministic fixture runtime was reset per dispatch.
- Severity: major
- Fix status: fixed
- Fix: `DispatchEngine` now keeps a per-engine atomic dispatch counter. The first dispatch remains `disp-0001` for existing deterministic expectations; subsequent dispatches in the same local engine process advance to `disp-0002`, `disp-0003`, and so on.
- Test reference: `successive_dispatches_from_one_engine_have_distinct_dispatch_ids` in `engine/src/dispatch_engine.rs`

### Finding 2 — Dashboard Dispatch Detail Rendered Empty Sections

- Flow tested: static dashboard dispatch detail.
- Expected behavior: clicking a dispatch row shows record, analysis, decision, execution, and evaluation sections.
- Actual behavior: the API returns dispatch detail as a history row with the dispatch bundle under `dispatch.bundle`, while the dashboard detail component looked for bundle sections at the top level.
- Severity: major
- Fix status: fixed
- Fix: dashboard detail now unwraps `detail.bundle` when present and renders the existing section layout from the bundle.
- Reference: `dashboard/src/components/Dispatches.tsx`

## Deferred Issues

None.

## Verification Status

Local pilot verification passed:

```bash
cargo build -p engine
uv run --no-project python scripts/trial4_real_use_pilot.py --output artifacts/trial4/pilot-results.json
uv run --no-project python scripts/trial4_real_use_pilot.py
```

Closeout verification run:

```bash
cargo fmt --check
cargo clippy -p engine -- -D warnings
cargo test -p engine
bash scripts/check_wire_codegen_drift.sh
uv run --no-project python tools/check_security_baseline.py
cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
cd sdk/typescript && ./node_modules/.bin/tsc -p tsconfig.json
cd sdk/typescript && node --test tests/client.test.mjs
cd dashboard && node scripts/lint-readonly.mjs
cd dashboard && ./node_modules/.bin/tsc -p tsconfig.json --noEmit
cd dashboard && node node_modules/next/dist/bin/next build
cd dashboard && node scripts/build-static.mjs
uv run --no-project python scripts/check_agent_handoff.py
```

Local Bun was unavailable, so `bash scripts/verify_rust_typescript_stack.sh` was not run locally. TypeScript SDK and dashboard build/test equivalents were run through the locally installed Node tooling.
