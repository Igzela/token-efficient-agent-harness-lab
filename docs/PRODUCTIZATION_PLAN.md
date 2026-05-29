# Productization Plan

Last updated: 2026-05-29.

This is the single roadmap for moving Agent Control Plane from local self-hosted MVP / internal beta toward a complete local-team control plane. Do not create parallel roadmap documents; update this file and keep `docs/NEXT_DECISION.md` as the short decision pointer.

## Current Product Level

**Local self-hosted MVP / internal beta.**

Already implemented:

- one Rust engine process can serve the API and static dashboard without Docker
- local SQLite persists dispatch history, config, team/API-key metadata, audit, costs, provider audit, and provider usage columns
- dashboard reads live local state
- TypeScript and Python SDKs cover local API, state, provider health/audit, export, and backup endpoints
- provider adapters exist as explicit env-gated beta paths; CI uses stub/mock paths and does not call real provider APIs
- target writes, sandbox/process/container/VM execution, real workers, cloud SaaS, and hosted production deployment remain out of scope

## Next Productization Phases

| Order | Phase | Goal | Done When |
|---|---|---|---|
| 1 | Provider Safety Gate | Make real provider execution safe, explicit, scoped, and auditable. | Provider execution requires explicit opt-in, auth, execute scope, startup safety summary, accurate dashboard state, and budget caps. |
| 2 | Permission Governance | Turn API-key metadata into manageable local team controls. | Roles, scopes, key creation/revocation/rotation, last-used tracking, and admin audit logs are available through API and dashboard. |
| 3 | Cost Governance | Make cost reporting match actual local behavior. | Dashboard and API separate reserved budget, provider-estimated cost, provider-reported usage, and audit-linked dispatch costs. |
| 4 | Data Operations | Make local state maintainable over time. | SQLite schema migrations, backup restore, import/export roundtrip tests, integrity checks, and data-directory docs are complete. |
| 5 | Native Packaging | Make no-Docker use installable without reading source. | Release artifact includes engine binary, dashboard assets, install/upgrade scripts, `.env.example`, and native smoke verification. |
| 6 | Dashboard Controls | Promote the dashboard from live viewer to local admin console without adding dangerous execution controls. | Admin-only config, backup/export, team/key, provider status, and dispatch-detail views exist with confirmations and audit logs. |
| 7 | Long-Run Hardening | Prepare for stable local-team use. | LAN threat model, audit integrity review, SQLite contention tests, provider failure matrix, upgrade smoke, and GitHub Actions Node deprecation cleanup are complete. |

## Phase 1 Scope: Provider Safety Gate

Do this first.

Required behavior:

- provider execution remains default-off
- real provider execution requires a separate explicit opt-in such as `ACP_ENABLE_PROVIDER_EXECUTION=1`
- provider execution requires `ACP_REQUIRE_AUTH=1`
- provider execution requires an execute scope, not only `dispatch:read`
- startup logs summarize provider type, auth state, host binding, budget cap, and whether LAN exposure is possible
- dashboard/API report the real provider state instead of always showing `stub/off`
- per-dispatch and local daily cost caps block execution before provider calls
- blocked provider execution returns a deterministic `not_executed` result and writes audit evidence

Minimum verification:

```bash
cargo fmt --check
cargo clippy -p engine -- -D warnings
cargo test -p engine
python3 scripts/smoke_native_runtime.py
python3 tools/check_security_baseline.py
python3 scripts/check_agent_handoff.py
```

## Keep Out Of Scope

- cloud/SaaS hosting
- provider execution enabled by default
- unattended provider execution
- target repository writes
- sandbox/process/container/VM execution
- real runtime workers
- hosted production deployment
- broad UI tracks unrelated to local-team operation
