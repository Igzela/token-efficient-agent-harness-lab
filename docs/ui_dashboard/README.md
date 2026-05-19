# UI Dashboard — Design Only

**Status:** Design document. No implementation. No Web UI. No server. No frontend dependencies.

## Scope

This directory contains design-only documentation for a future read-only observability dashboard. The dashboard displays harness state — it does not create, mutate, or activate state.

## What this is

- A set of design documents defining data contracts, interaction policies, security rules, and entry criteria for a future dashboard.
- A commitment to **read-only first**: the first version displays state, never mutates it.

## What this is NOT

- **Not a Web UI.** No React, Vue, Svelte, Next.js, Vite, Flask, Express, or any frontend framework.
- **Not a server.** No HTTP server, no API endpoint, no network listener.
- **Not an implementation.** No source code, no build pipeline, no `npm install`, no `pip install`.
- **Not a feature.** No policy activation, no provider calls, no sandbox execution, no PR creation.

## Files

| File | Purpose |
|------|---------|
| `UI_DASHBOARD_DESIGN.md` | Goals, non-goals, 11 dashboard views, first implementation mode |
| `DASHBOARD_DATA_CONTRACT.md` | 20+ data sources, schemas, redaction rules |
| `READ_ONLY_INTERACTION_POLICY.md` | Forbidden/allowed actions, human approval rules |
| `DASHBOARD_SECURITY_AND_PRIVACY.md` | Redaction, display policy, local-only enforcement |
| `DASHBOARD_ENTRY_CRITERIA.md` | Prerequisites, allowed/forbidden first steps |

## Design Principles

1. **Read-only first.** The dashboard displays state, does not create state.
2. **No server.** First implementation is a static report or local HTML file opened in a browser.
3. **No telemetry.** No analytics, no upload, no external calls.
4. **Human approval required.** Any write action (future) requires explicit human sign-off.
5. **Sensitive data redacted.** Credentials, secrets, raw provider responses are never displayed.

## Next Steps

After all entry criteria are met (see `DASHBOARD_ENTRY_CRITERIA.md`), the first implementation may proceed as a static local report. Server-based and interactive modes require separate design and approval cycles.
