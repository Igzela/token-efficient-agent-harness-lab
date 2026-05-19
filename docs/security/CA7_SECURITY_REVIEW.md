# CA-7 Security Review — Executive Summary

Date: 2026-05-19
Baseline commit: `aedcc81`
Reviewer: Automated security review (Claude Code)
Status: Complete — no critical blockers

---

## Executive Summary

This review assessed the Token-Efficient Agent Harness Lab at its CA-7 sealed
baseline. The sealed baseline operates in a read-only, no-credentials,
no-network-access configuration. All model interactions are mocked, all
governance decisions are fixture-based, and no external system calls occur.

**No critical blockers for maintaining the CA-7 sealed baseline were found.**

The codebase demonstrates strong separation between diagnostic and active
policy paths, enforces multi-gate governance for policy activation, and
maintains a clean audit trail via `events.jsonl`. The 12 security controls
identified are all active and test-covered.

**This is not a production security certification.** The sealed baseline
operates under a fundamentally different trust model than a production
deployment. The findings and controls documented here are specific to the
current stage-0 configuration.

---

## Scope

| In Scope | Details |
|----------|---------|
| Source code | `src/harness_core/` — all Python modules |
| Test suite | `tests/` — all test files and fixtures |
| Configuration | JSON fixtures under `tests/fixtures/` |
| Event log | `docs/stage0/events.jsonl` |
| Documentation | `docs/` — architecture, governance, policy candidate lifecycle |
| Dependencies | Standard library + pytest (no additional packages installed) |

---

## Out of Scope

| Item | Reason |
|------|--------|
| Real model provider integration | Not present at CA-7; requires CA-8 review |
| Credential management | No credentials exist; requires CA-8 review |
| Sandbox execution | No sandbox implemented; requires dedicated review |
| Production deployment | Not applicable at sealed baseline stage |
| Network security | No external network access at CA-7 |
| Supply chain audit | Dependencies not locked; requires separate audit |

**CA-8, real provider integration, sandbox execution, and productionization
require separate security review.**

---

## Findings

### F-001: No Critical Findings

No credential leakage, accidental provider calls, or trust boundary
violations were detected. The sealed baseline maintains a clean security
posture within its scope.

### F-002: Static Analysis Coverage

The secret scan and import scan cover all git-tracked files. The scans use
pattern matching (regex for secrets, AST for imports) and are deterministic.
Placeholder strings (e.g., `"your-api-key-here"`) are explicitly excluded to
avoid false positives.

### F-003: Governance Gate Integrity

All five governance gates (evidence, approval, rollback, scope, unknown-error)
are enforced in code and validated by test fixtures covering both pass and fail
paths. No bypass path exists in the sealed baseline.

### F-004: Audit Trail Preservation

`events.jsonl` is committed and tracked. The stage-0 event guard in the
security checker verifies its existence and basic integrity. No code path
modifies the event log outside of explicit event recording.

---

## Controls Summary

| Control Category | Count | Status |
|------------------|-------|--------|
| Network isolation | 3 (SEC-001, SEC-003, SEC-004) | All active |
| Credential protection | 2 (SEC-002, SEC-011) | All active |
| Governance enforcement | 5 (SEC-005–SEC-009) | All active |
| Audit trail | 1 (SEC-010) | Active |
| Static analysis | 1 (SEC-012) | Active |
| **Total** | **12** | **All active** |

---

## Checker Summary

The automated security baseline checker (`tools/check_security_baseline.py`)
performs five checks:

| Check | Description | Result |
|-------|-------------|--------|
| Secret scan | Regex scan for credential patterns in git-tracked files | PASS |
| Import scan | AST-based scan for prohibited network/SDK imports | PASS |
| Active routing guard | Scan for `active_routing_allowed: true` in JSON | PASS |
| Governance boundary guard | Verify governance fixtures exist and are well-formed | PASS |
| Stage-0 event guard | Verify `events.jsonl` exists and is intact | PASS |

---

## Residual Risks

| ID | Risk | Severity | Mitigation Plan |
|----|------|----------|-----------------|
| RR-001 | No runtime secret scanning | Medium | Add env-var scanning in CA-8 |
| RR-002 | No sandbox escape testing | Medium | Add escape tests when sandbox is implemented |
| RR-003 | Fixture staleness | Low | Regular fixture refresh in CA-8 |
| RR-004 | No supply-dependency audit | Medium | Add dependency scanning and lockfile in CA-8 |
| RR-005 | Human approval is conceptual | Low | Implement approval workflow in CA-8 |

---

## Recommendation

Maintain the CA-7 sealed baseline as-is. The security posture is appropriate
for the current stage. When advancing to CA-8:

1. Re-run this review after real provider integration to assess new attack
   surface.
2. Implement runtime secret detection before introducing credentials.
3. Design sandbox with escape-test coverage before enabling code execution.
4. Add dependency lockfile and vulnerability scanning.
5. Implement human-approval workflow before enabling policy activation.
