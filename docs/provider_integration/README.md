# Provider Integration Design Directory

**Status**: Design-only — no implementation.

This directory contains design documents for future real provider integration. These designs do NOT constitute provider接入 (provider integration) work and do NOT activate CA-8.

## Scope

- Design documents only: architecture, contracts, policies, entry criteria
- No real provider connections, no SDK imports, no API key reads
- No model calls, no runtime changes, no events.jsonl modifications
- No new dependencies

## What This Is NOT

- This is not the CA-8 Provider Integration milestone
- No code changes ship from these designs
- These documents are proposals subject to review and approval

## Files

| Document | Purpose |
|----------|---------|
| `REAL_PROVIDER_INTEGRATION_DESIGN.md` | Provider integration goals, lifecycle, failure handling, audit requirements |
| `CREDENTIAL_AND_SECRET_POLICY.md` | Credential management and secret handling policies |
| `PROVIDER_REQUEST_RESPONSE_CONTRACT.md` | Future schema design for provider requests and responses |
| `BUDGET_AND_USAGE_POLICY.md` | Budget controls, usage accounting, and cost policies |
| `CA8_ADVISOR_ONLY_ENTRY_CRITERIA.md` | Preconditions, allowed/forbidden actions, exit criteria for CA-8 |

## Design Principles

1. **Diagnostic evidence only**: Provider responses are used for diagnosis, not policy activation
2. **Fail closed**: No credential available means no call made
3. **Audit everything**: Every request gets a unique ID; no raw secrets logged
4. **Advisor-only entry**: CA-8 begins as read-only advisor, not active agent
