# Threat Model — CA-7 Sealed Baseline

Date: 2026-05-19
Baseline commit: `aedcc81`
Scope: Entire repository in its sealed baseline configuration

---

## 1. Assets

| Asset | Description | Sensitivity |
|-------|-------------|-------------|
| `events.jsonl` | Canonical stage-0 event log; source of truth for sealed baseline | High — tampering breaks audit trail |
| `tests/fixtures/` | Test data including governance decisions, model profiles, tool error cases, policy candidates, context packs | Medium — fixture integrity validates correctness |
| Governance policy records | JSON fixtures encoding approval/rollback/scope/unknown-error gate decisions | High — represent the trust decisions the system enforces |
| Model profiles | Shadow routing configs, admission thresholds, tier maps | Medium — incorrect profiles could enable unintended routing |
| Context packs | Sealed reference context bundles used in evaluations | Medium — mutation could distort evaluation results |
| Usage ledger | Records of harness usage runs and their outcomes | Medium — integrity required for policy candidate lifecycle |
| Source code (`src/harness_core/`) | Core harness logic: governance, routing, evaluation, scoring, orchestration | High — controls all system behavior |
| Future credentials | Not present at CA-7; API keys, tokens, service accounts that will be introduced in CA-8+ | Critical when introduced |

---

## 2. Trust Boundaries

| Boundary | Inside (trusted) | Outside (untrusted) | Control |
|----------|-------------------|----------------------|---------|
| Repository | All committed source, docs, fixtures | Remote origin, CI/CD, contributors | Git commit history, branch protection |
| Fixture boundary | Test fixtures under `tests/fixtures/` | User-provided or external data | Fixture validation in tests |
| Context pack boundary | Sealed context packs in fixtures | Runtime context from external sources | Sealed baseline — no external context loaded |
| Governance boundary | Governance gate logic in `governance.py` | Any code that bypasses gate checks | Gate enforcement in policy candidate lifecycle |
| Future provider boundary | Internal harness | Model provider APIs (Anthropic, OpenAI, etc.) | **Not yet active** — no real calls at CA-7 |
| Future sandbox boundary | Sandboxed execution environment | Host filesystem, network, processes | **Not yet active** — no sandbox at CA-7 |
| Human approval boundary | Automated harness pipeline | Human reviewer decisions | Human approval required for activation |

---

## 3. Threats

### T-001: Credential Leakage

**Description:** API keys, tokens, or service account credentials are committed
to the repository, logged in events, or exposed in test output.

**Impact:** High — credential compromise enables unauthorized model provider
access, data exfiltration.

**Current status:** No credentials exist in the sealed baseline. The codebase
does not import `os.environ` for API keys, does not contain `.env` files, and
no provider SDK is installed.

---

### T-002: Accidental Provider Call

**Description:** Code makes an outbound HTTP request to a model provider API,
incurring cost or leaking data.

**Impact:** High — unexpected API calls generate costs and may transmit
fixture/production data to external services.

**Current status:** No HTTP client libraries (`requests`, `httpx`, `aiohttp`,
`urllib.request`) are imported. No provider SDKs (`openai`, `anthropic`,
`google-generativeai`) are present. The `model_gateway.py` module defines
interfaces but makes no real calls.

---

### T-003: Diagnostic → Active Policy Promotion

**Description:** A diagnostic-only policy candidate is accidentally promoted
to active status, changing routing behavior without governance review.

**Impact:** Medium — incorrect routing could degrade evaluation quality or
introduce bias.

**Current status:** The governance system requires explicit `approve_activation`
decisions with evidence, approval, rollback, scope, and unknown-error gates.
Diagnostic evidence is tracked separately from admitted evidence.

---

### T-004: Active Routing Enabled

**Description:** `active_routing_allowed: true` appears in a model profile or
configuration, enabling live traffic routing.

**Impact:** High — active routing in a non-production environment could cause
unintended model selection.

**Current status:** No configuration file contains `active_routing_allowed:
true`. The sealed baseline uses shadow routing only.

---

### T-005: Unknown Error Marked Retryable

**Description:** An error classified as `unknown` is given a retryable
classification, causing infinite retry loops or unexpected behavior.

**Impact:** Medium — could cause resource exhaustion or mask real failures.

**Current status:** The error taxonomy distinguishes `unknown` from retryable
errors. Governance gates check for unknown-error classification consistency.

---

### T-006: Context Overexposure

**Description:** Context packs contain more information than necessary,
exposing sensitive data to the model or evaluation pipeline.

**Impact:** Medium — unnecessary data exposure increases blast radius of
context pack compromise.

**Current status:** Context packs at CA-7 are sealed test fixtures. No PII or
real credentials are present. Overexposure is a concern for CA-8 when real
context is introduced.

---

### T-007: Prompt Mutation

**Description:** System prompts or evaluation prompts are modified without
governance review, changing model behavior.

**Impact:** Medium — prompt changes can alter model outputs, bypass safety
checks, or introduce bias.

**Current status:** Prompts are part of sealed fixtures. No runtime prompt
construction exists at CA-7. Mutation tracking is a CA-8 concern.

---

### T-008: Sandbox Escape

**Description:** Code executing in a sandboxed environment escapes to the host
filesystem or network.

**Impact:** Critical — full host compromise.

**Current status:** No sandbox exists at CA-7. The `sandbox.py` module defines
interfaces only. No code execution occurs outside the test runner.

---

### T-009: File Mutation

**Description:** The harness modifies source code, fixtures, or configuration
files outside of explicit git-tracked changes.

**Impact:** Medium — silent file changes break reproducibility and audit trail.

**Current status:** The harness operates read-only on the codebase at CA-7.
File mutations are a concern for CA-8 when the harness may write artifacts.

---

### T-010: Rollback Missing

**Description:** A policy activation is applied but the rollback plan is
incomplete or untested, preventing safe revert.

**Impact:** Medium — inability to revert could leave the system in a broken
state.

**Current status:** Governance gates include a rollback gate that verifies
rollback plan references. Rollback plans are required for `approve_activation`
decisions.

---

## 4. Existing Controls

| ID | Control | Addresses |
|----|---------|-----------|
| C-001 | No real model API calls — all providers are mocked or stubbed | T-002 |
| C-002 | No credentials in repository — no `.env`, no API keys in source | T-001 |
| C-003 | No external network access — no HTTP client libraries imported | T-002 |
| C-004 | No active routing — `active_routing_allowed` is never `true` | T-004 |
| C-005 | No sandbox execution — `sandbox.py` is interface-only | T-008 |
| C-006 | No diagnostic activation — diagnostic evidence is tracked separately | T-003 |
| C-007 | Human approval required — governance gates enforce approval step | T-003, T-010 |
| C-008 | Rollback required — governance gates verify rollback plan exists | T-010 |
| C-009 | `events.jsonl` preserved — sealed baseline event log is committed and tracked | T-009 |
| C-010 | Test suite — 751 tests validate governance, routing, evaluation, and scoring logic | All |
| C-011 | Secret scan — `check_security_baseline.py` scans for credential patterns | T-001 |
| C-012 | Import scan — AST-based check for prohibited network/SDK imports | T-002 |

---

## 5. Residual Risks

### RR-001: No Runtime Secret Scanning

The secret scan is static (file-level). There is no runtime detection of
secrets entering the system through environment variables or dynamic
configuration. **Mitigated in CA-8** by adding environment-variable scanning
and runtime secret detection.

### RR-002: No Sandbox Escape Testing

The sandbox interface exists but is untested against escape vectors. No
fuzzing or adversarial testing has been performed. **Mitigated when sandbox
is implemented** by adding escape-test coverage.

### RR-003: Fixture Staleness

Test fixtures represent a point-in-time snapshot. As the codebase evolves,
fixtures may become stale and no longer exercise current code paths.
**Mitigated** by regular fixture refresh as part of CA-8 development.

### RR-004: No Supply-Dependency Audit

Third-party Python packages (e.g., `pytest`) are not audited for
vulnerabilities. The sealed baseline does not lock dependency versions in a
lockfile. **Mitigated in CA-8** by adding dependency scanning and lockfile
enforcement.

### RR-005: Human Approval is Conceptual

The governance system requires human approval, but at CA-7 this is a data
field in JSON fixtures — there is no actual human-in-the-loop mechanism.
**Mitigated in CA-8** by implementing approval workflow integration.
