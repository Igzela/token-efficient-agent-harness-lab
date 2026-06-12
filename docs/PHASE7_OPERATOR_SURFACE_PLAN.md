# Phase 7: Operator Surface / UI & UX

Status: **SCOPE LOCK**
Plan created: 2026-06-12
Schema version: 14
Test count baseline: 1534 Rust tests, 0 failures
Dashboard baseline: 35 components, plain CSS with oklch design tokens, light/dark mode, Next.js 15 App Router, React 19, TypeScript 5.7, Bun

## Purpose

Phase 7 hardens the read-only operator dashboard so a human operator can understand the full regulator, proposal, snapshot, and dispatch state at a glance. It adds no mutation controls, no new runtime behavior, and no boundary expansion. The goal is calm, honest, accessible visibility into the system that already exists.

## Phase 7 Goals

| # | Goal | Acceptance Criterion |
|---|---|---|
| 1 | Read-only operator dashboard | All new dashboard surfaces are GET-only; no POST/PUT/DELETE calls from new UI; readonly lint guard passes |
| 2 | Regulator state visualization | `GET /api/v1/regulator/state` response rendered with mode indicator, env-gate badges, proposal counts, auto-adjustment summary, warnings, and active routing policy |
| 3 | Proposal/snapshot visibility | Pending/active proposals listed with status, tier overrides, confidence, created-at, and human-readable summaries; policy snapshots visible with timestamp and diff |
| 4 | Empty/loading/error states | Every new data surface has: loading spinner, empty state with actionable guidance, error state with message and retry |
| 5 | Responsive layout | Dashboard usable at 1024px+ without horizontal scroll; key tiles and tables reflow at common breakpoints |
| 6 | Accessibility | Keyboard navigation, visible focus states, ARIA roles/labels, screen-reader-friendly structure |
| 7 | Honest wording | No marketing language, no enterprise jargon; labels describe what the system actually does |

## Non-Goals

The following are explicitly NOT Phase 7 work:

| # | Non-Goal | Reason |
|---|---|---|
| 1 | Mutation controls (approve/reject/rollback/deactivate from UI) | Read-only surface; existing API mutation endpoints remain backend-only |
| 2 | POST UI for proposals or auto-adjustments | No new write paths in dashboard |
| 3 | Policy mutation from dashboard | Policy mutation requires explicit human approval via API/CLI |
| 4 | Batch auto-apply | Not approved; requires separate implementation plan |
| 5 | Daemonized loops | No background scheduling or auto-refresh loops beyond existing 60s visibility-aware refresh |
| 6 | Target repo writes | App never writes to target repos |
| 7 | Provider/auth/security/deploy boundary expansion | Requires explicit human approval |
| 8 | New database migrations unless justified | Current schema v14 sufficient; any new migration needs documented reason |
| 9 | Phase 8 work | Out of scope |
| 10 | PostgreSQL success claims | PG active trial is BLOCKED (`ACP_TEST_DATABASE_URL` not available) |
| 11 | New runtime/API endpoints | Phase 7 adds dashboard UI only; existing endpoints are sufficient |
| 12 | Component library migration (shadcn-ui install, Radix, Tailwind) | Phase 7 adopts shadcn-ui aesthetic discipline in plain CSS; no dependency changes |

## UI Aesthetic Principles

The existing dashboard uses plain CSS with oklch design tokens (`globals.css`, ~1197 lines) and a light/dark color scheme. Phase 7 follows these principles without adding a component library dependency:

| Principle | Guideline |
|---|---|
| shadcn-ui component discipline | Components are small, focused, composable; props are explicit; no god-components; each component owns one concern |
| Sparse motion | CSS transitions at 150ms for hover/focus only; no animation on data change; no loading skeletons with shimmer |
| Monochrome/low-saturation | Existing oklch palette is low-saturation by design; new surfaces use the same `--ink`, `--muted`, `--border`, `--bg` tokens |
| Thin borders | `1px solid var(--border)` everywhere; no decorative borders; no card shadows beyond `--shadow-sm` |
| Generous whitespace | Padding at `1rem` or `1.5rem`; sections separated by `2rem` gaps; no cramped layouts |
| Calm dark mode | Dark mode inherits from `@media (prefers-color-scheme: dark)` existing tokens; no neon, no glow, no high-contrast overrides |
| No marketing gloss | Labels say "Regulator" not "Intelligence Engine"; "Proposals" not "Smart Policy Recommendations"; "Adjustments" not "Auto-Healing" |
| No enterprise clutter | No badges, ribbons, progress rings, or status dots unless they carry real operational meaning |

## Read-Only Safety Boundary

Phase 7 dashboard changes are bound by the existing readonly lint guard (`dashboard/scripts/lint-readonly.mjs`):

- **Forbidden words in dashboard source**: `approve`, `deploy`, `execute`, `merge`, `run` (enforced at build time)
- **Forbidden patterns**: `dispatch()` calls, POST/PUT/DELETE to API endpoints
- **Allowed**: GET requests to existing read-only API endpoints
- **Enforcement**: `node scripts/lint-readonly.mjs` runs in CI and in `verify_rust_typescript_stack.sh`

Any new dashboard component that attempts a mutation will fail lint and CI. Phase 7 adds no exceptions to this guard.

## Allowed Data Sources / Endpoints

Phase 7 dashboard reads only from these existing GET endpoints:

### Primary for Phase 7

| Endpoint | Purpose | Response Shape |
|---|---|---|
| `GET /api/v1/regulator/state` | Full regulator operational snapshot | `{ schema_version, regulator: { mode, env_gate_enabled, dry_run_enabled, active_gate_enabled, pg_database_url_configured }, active_routing_policy, proposals: { pending_count, active_count }, auto_adjustments: { active_count, report }, warnings }` |
| `GET /api/v1/proposals?status=&limit=&offset=` | Policy proposals list | `{ schema_version, proposals, total, limit, offset }` |
| `GET /api/v1/proposals/:id` | Single proposal detail | `{ schema_version, proposal }` |
| `GET /api/v1/proposals/generated` | Generated proposal candidates | `{ schema_version, candidates }` |
| `GET /api/v1/auto-adjustments` | Auto-adjustment records | `{ schema_version, adjustments, total }` |
| `GET /api/v1/simulation/policy-delta?policy=&limit=` | Policy simulation delta | Policy simulation result with success_rate_delta, cost_delta, latency_delta, human_review_rate_delta |
| `GET /api/v1/simulation/report?limit=` | Shadow routing simulation | `{ schema_version, report }` |

### Existing dashboard endpoints (unchanged)

| Endpoint | Purpose |
|---|---|
| `GET /api/v1/health` | Engine health |
| `GET /api/v1/ready` | Readiness |
| `GET /api/v1/dashboard` | Dashboard summary state |
| `GET /api/v1/dispatches?limit=&offset=&search=` | Dispatch history |
| `GET /api/v1/dispatches/:id` | Dispatch detail |
| `GET /api/v1/dispatch-metrics?limit=` | Dispatch metrics |
| `GET /api/v1/feedback/traces?limit=&offset=` | Feedback traces |
| `GET /api/v1/feedback/patterns?limit=` | Feedback patterns |
| `GET /api/v1/feedback/cost-of-pass?limit=` | Cost-of-pass |
| `GET /api/v1/metrics` | Operations metrics |
| `GET /api/v1/metrics/observability` | Observability metrics |
| `GET /api/v1/proposals?status=pending` | Pending proposals |
| `GET /api/v1/decision-log?limit=&offset=` | Decision log |
| `GET /api/v1/executor-pool` | Executor pool status |
| `GET /api/v1/queue/status` | Queue status |
| `GET /api/v1/plans` | Workflow plans |
| `GET /api/v1/workflow-runs` | Workflow runs |
| `GET /api/v1/supervised-patch/workspaces` | Supervised patch workspaces |
| `GET /api/v1/supervised-patch/artifacts` | Supervised patch artifacts |
| `GET /api/v1/audit?limit=&offset=&search=` | Audit log |
| `GET /api/v1/backups` | Backups |
| `GET /api/v1/costs` | Cost summary |
| `GET /api/v1/keys` | API keys (metadata only) |
| `GET /api/v1/team` | Team members |
| `GET /api/v1/config` | Configuration |

## Validation Plan

All commands run from repository root. Every command must pass before merge.

### Rust (engine)

```
cargo fmt --check
cargo clippy -p engine -- -D warnings
cargo test -p engine
```

### TypeScript SDK

```
cd sdk/typescript && bun run build && bun run test
```

### Dashboard

```
cd dashboard && bun run lint        # readonly lint guard
cd dashboard && bun run typecheck   # TypeScript strict
cd dashboard && bun run build       # Next.js production build
cd dashboard && bun run build:static # static export verification
```

### Integration

```
bash scripts/verify_rust_typescript_stack.sh  # full stack verification
bash scripts/check_wire_codegen_drift.sh      # wire type drift
uv run --no-project python scripts/check_agent_handoff.py  # handoff guard
```

### CI Jobs (GitHub Actions)

| Job | Command |
|---|---|
| `rust-tests` | `cargo test -p engine`, `cargo fmt --check`, `cargo clippy -p engine -- -D warnings` |
| `typescript-tests` | `cd dashboard && bun run lint && bun run typecheck && bun run build` |
| `native-runtime` | `bash scripts/verify_rust_typescript_stack.sh` |
| `python-tests` | `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python tools/check_security_baseline.py` |

## Accessibility Requirements

| # | Requirement | Verification |
|---|---|---|
| 1 | Keyboard navigation | All interactive elements reachable via Tab; tab panels navigable with arrow keys |
| 2 | Visible focus states | `:focus-visible` ring on all buttons, links, inputs, tabs using `outline: 2px solid var(--accent)` |
| 3 | ARIA roles | `role="tablist"`, `role="tab"`, `role="tabpanel"` on tab navigation; `role="dialog"`, `aria-modal` on modals; `role="status"` on loading indicators |
| 4 | ARIA labels | `aria-label` on icon-only buttons; `aria-labelledby` on sections; `aria-live="polite"` on data regions that update |
| 5 | Screen reader structure | Semantic HTML (`<section>`, `<nav>`, `<table>`, `<th scope>`); no layout tables; meaningful heading hierarchy (h1 > h2 > h3) |
| 6 | Color contrast | Text meets WCAG AA (4.5:1 for body, 3:1 for large text); status conveyed by text/icon, not color alone |
| 7 | Form labels | Every `<input>` has `<label htmlFor>` or `aria-label` |

## Acceptance Criteria

Phase 7 is accepted when ALL of the following hold:

| # | Criterion | Verification |
|---|---|---|
| 1 | All Phase 7 PRs merged to main | `git log --oneline` shows PRs #49-#54 |
| 2 | CI green | `gh run list --limit 3` shows all passing |
| 3 | Rust tests pass | `cargo test -p engine` -- 1534+ tests, 0 failures |
| 4 | Dashboard readonly lint | `cd dashboard && bun run lint` passes |
| 5 | Dashboard typecheck | `cd dashboard && bun run typecheck` passes |
| 6 | Dashboard build + static export | `cd dashboard && bun run build && bun run build:static` passes |
| 7 | Regulator state visualized | `GET /api/v1/regulator/state` response rendered in dashboard with mode, gates, proposals, adjustments, warnings |
| 8 | Proposals visible | Pending/active/generated proposals displayed with status, details, and empty states |
| 9 | Snapshots visible | Policy snapshots accessible from regulator or proposals view |
| 10 | Empty/loading/error states | Every new data surface has all three states |
| 11 | Responsive layout | No horizontal scroll at 1024px+; tables scroll horizontally at narrow widths |
| 12 | Accessibility | Keyboard nav works; focus visible; ARIA roles present; screen-reader structure |
| 13 | Honest wording | No marketing language; labels match system behavior |
| 14 | Handoff guard | `uv run --no-project python scripts/check_agent_handoff.py` passes |
| 15 | No mutation in dashboard | `node scripts/lint-readonly.mjs` passes; no POST/PUT/DELETE calls in new code |

## Final Seal Criteria

Phase 7 is sealed when:

1. All 6 PRs (#49-#54) merged to main with green CI
2. `docs/CURRENT_STATUS.md` updated with Phase 7 DONE
3. `docs/NEXT_DECISION.md` updated with next phase direction
4. `docs/MODULE_MAP.md` updated if any new modules added
5. Test count confirmed (Rust and dashboard)
6. No boundary expansion: no new env vars, no new auth scopes, no new provider/CLI paths, no new DB migrations
7. `uv run --no-project python scripts/check_agent_handoff.py` passes
8. `bash scripts/verify_rust_typescript_stack.sh` passes

## PR Sequence

### PR #49 — Phase 7 Scope Lock

- **Branch**: `phase7/scope-lock`
- **Purpose**: This document + acceptance checklist
- **Risk**: Low (docs-only)
- **Type**: docs-only
- **Files**: `docs/PHASE7_OPERATOR_SURFACE_PLAN.md`, `docs/NEXT_DECISION.md`

### PR #50 — Regulator State Visualization

- **Branch**: `phase7/regulator-state-viz`
- **Purpose**: Wire `GET /api/v1/regulator/state` into the dashboard Regulator tab; add `fetchRegulatorState()` to api-client; render mode indicator (disabled/dry_run/active), env-gate badges, proposal counts, auto-adjustment summary, warnings list, and active routing policy section
- **Risk**: Low (dashboard-only, read-only GET)
- **Type**: dashboard UI
- **Files**: `dashboard/src/lib/api-client.ts`, `dashboard/src/lib/types.ts`, `dashboard/src/components/DynamicRegulator.tsx` (or new `RegulatorState.tsx`)
- **Verification**: `cd dashboard && bun run lint && bun run typecheck && bun run build && bun run build:static`

### PR #51 — Proposal/Snapshot Read-Only Surfaces

- **Branch**: `phase7/proposals-snapshots`
- **Purpose**: Enhance proposals display with full detail (tier overrides, confidence, status transitions, timestamps); add generated proposals section; add policy snapshot preview when available; ensure all proposal actions (approve/reject/rollback/deactivate) remain API-only with no new dashboard mutation calls
- **Risk**: Low (dashboard-only, read-only GET)
- **Type**: dashboard UI
- **Files**: `dashboard/src/components/DynamicRegulator.tsx`, `dashboard/src/lib/types.ts`, `dashboard/src/lib/api-client.ts`
- **Verification**: `cd dashboard && bun run lint && bun run typecheck && bun run build && bun run build:static`

### PR #52 — Empty/Loading/Error States + Honest Wording

- **Branch**: `phase7/states-wording`
- **Purpose**: Audit all regulator/proposal/snapshot data surfaces for empty/loading/error coverage; add `EmptyState` components with actionable guidance; add loading spinners with `role="status"` and `aria-live="polite"`; add error states with retry buttons; replace any marketing language with honest operational labels
- **Risk**: Low (dashboard-only, no behavior change)
- **Type**: dashboard UI polish
- **Files**: `dashboard/src/components/DynamicRegulator.tsx`, `dashboard/src/components/EmptyState.tsx`, `dashboard/src/components/StateBanner.tsx`, `dashboard/src/app/globals.css`
- **Verification**: `cd dashboard && bun run lint && bun run typecheck && bun run build && bun run build:static`

### PR #53 — Responsive Layout + Accessibility

- **Branch**: `phase7/responsive-a11y`
- **Purpose**: Ensure regulator/proposal tables reflow at 1024px+; add `:focus-visible` rings; add ARIA roles (`role="tablist"`, `role="tab"`, `role="tabpanel"`, `role="dialog"`, `aria-modal`, `role="status"`); add `aria-label` on icon buttons; verify heading hierarchy; add `aria-live="polite"` on data regions
- **Risk**: Low (dashboard-only, CSS + ARIA attributes)
- **Type**: dashboard accessibility
- **Files**: `dashboard/src/app/globals.css`, `dashboard/src/components/DynamicRegulator.tsx`, `dashboard/src/components/TabGroup.tsx`, `dashboard/src/components/ConfirmDialog.tsx`
- **Verification**: `cd dashboard && bun run lint && bun run typecheck && bun run build && bun run build:static`

### PR #54 — Phase 7 Final Seal

- **Branch**: `phase7/final-seal`
- **Purpose**: Seal Phase 7 as DONE; update `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`; confirm test counts; run full verification suite
- **Risk**: Low (docs-only)
- **Type**: docs-only
- **Files**: `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `docs/PHASE7_OPERATOR_SURFACE_PLAN.md`
- **Verification**: `uv run --no-project python scripts/check_agent_handoff.py`, `bash scripts/verify_rust_typescript_stack.sh`

## Dashboard Stack Reference

| Layer | Technology | Version |
|---|---|---|
| Framework | Next.js App Router | 15.3 |
| UI Library | React | 19.1 |
| Language | TypeScript | 5.7 |
| Package Manager | Bun | latest |
| Styling | Plain CSS with oklch tokens | `globals.css` (~1197 lines) |
| Component Pattern | Function components, hooks | No class components |
| State Management | React useState/useEffect | No external state lib |
| API Client | fetch-based `api-client.ts` | Typed with TypeScript interfaces |
| Type Source | Hand-maintained `types.ts` + generated SDK wire types | `api-types.ts` pattern |
| Build | `bun run build` (Next.js) + `bun run build:static` (static export) | |
| Lint | `node scripts/lint-readonly.mjs` | Forbidden words: approve, deploy, execute, merge, run |
| Deployment | Static export served by Rust engine via `ACP_DASHBOARD_DIR` | No separate dashboard server |

## Design Token Reference

The existing `globals.css` defines these tokens (Phase 7 uses only these):

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | oklch(0.985) | oklch(0.155) | Page background |
| `--bg-subtle` | oklch(0.965) | oklch(0.185) | Section backgrounds |
| `--panel` | oklch(1) | oklch(0.20) | Card/panel backgrounds |
| `--ink` | oklch(0.175) | oklch(0.935) | Primary text |
| `--ink-subtle` | oklch(0.50) | oklch(0.65) | Secondary text |
| `--muted` | oklch(0.55) | oklch(0.55) | Disabled/placeholder |
| `--border` | oklch(0.905) | oklch(0.28) | Standard borders |
| `--border-strong` | oklch(0.82) | oklch(0.35) | Emphasis borders |
| `--accent` | oklch(0.47 0.10 210) | oklch(0.65 0.10 210) | Interactive elements |
| `--ok` | oklch(0.50 0.12 155) | oklch(0.50 0.12 155) | Success |
| `--warn` | oklch(0.55 0.13 75) | oklch(0.55 0.13 75) | Warning |
| `--risk` | oklch(0.52 0.16 25) | oklch(0.52 0.16 25) | Error/danger |

## Regulator State Response Shape

The `GET /api/v1/regulator/state` endpoint (implemented in `engine/src/http_server/handlers/dispatch.rs`, function `api_regulator_state`) returns:

```json
{
  "schema_version": "regulator_state.v1",
  "regulator": {
    "mode": "disabled" | "dry_run" | "active",
    "env_gate_enabled": true | false,
    "dry_run_enabled": true | false,
    "active_gate_enabled": true | false,
    "pg_database_url_configured": true | false
  },
  "active_routing_policy": { ... } | null,
  "proposals": {
    "pending_count": 0,
    "active_count": 0
  },
  "auto_adjustments": {
    "active_count": 0,
    "report": { ... }
  },
  "warnings": ["..."]
}
```

Phase 7 renders this response as a structured panel in the Regulator tab with:
- Mode badge (disabled = grey, dry_run = amber, active = green)
- Env-gate status indicators (checkmark/x for each gate)
- Proposal count tiles
- Auto-adjustment summary
- Warnings list with `role="alert"`
- Active routing policy section (if present)
- PG status note (BLOCKED warning when `pg_database_url_configured` is true)
