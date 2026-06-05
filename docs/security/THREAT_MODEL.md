# Threat Model — Local Agent Control Plane

Last updated: 2026-06-05
Scope: Rust engine, TypeScript dashboard/SDK, local SQLite state, env-gated provider adapters, Batch 6 supervised-execution design-gate risks, Batch 7 Slice A storage-only supervised patch metadata, and Batch 7 Slice B read-only HTTP metadata views. Batch 6/7 risks are not implemented runtime features.

---

## 1. Assets

| Asset | Description | Sensitivity |
|-------|-------------|-------------|
| Local SQLite database | App-owned dispatch history, config, team/API-key metadata, audit log, cost state | High — contains all local operational state |
| API keys | Scoped local authentication tokens for the control plane API | Critical — compromise enables unauthorized dispatch and state mutation |
| Team metadata | User IDs, roles, scopes | Medium — controls access boundaries |
| Provider credentials | `ACP_API_KEY` and related env vars for real model providers | Critical — enables real API calls and cost exposure |
| Dispatch bundles | Task analysis, routing decisions, execution results | Medium — operational intelligence |
| Audit log | Immutable record of all state mutations | High — tampering breaks accountability |
| Source code (Rust `engine/`, TypeScript `dashboard/`, `sdk/`) | All runtime logic | High — controls all system behavior |
| Static dashboard export | Pre-built Next.js UI served by the engine | Low — read-only interface |
| Future target workspace | Planned app-owned detached patch workspace/snapshot area for any later supervised patch artifact beta | Critical — Slice A stores metadata only and Slice B exposes read-only metadata views; runtime workspace creation not implemented |
| Future execution artifacts | Planned patch artifacts, diffs, evidence manifests, rollback/quarantine evidence, and captured files | High — Slice A stores metadata only and Slice B exposes read-only metadata views; patch file capture/redaction/export not implemented |

---

## 2. Trust Boundaries

| Boundary | Inside (trusted) | Outside (untrusted) | Control |
|----------|-------------------|----------------------|---------|
| Local host | Engine process, SQLite file, dashboard | Network peers, remote APIs | Process isolation, localhost binding |
| API auth boundary | Authenticated requests with valid API keys | Unauthenticated or expired/revoked requests | `TenantResolver`, scope checks, 401/403 responses |
| Provider boundary | Internal dispatch engine | Real model provider APIs (OpenAI, Anthropic) | `ACP_ENABLE_PROVIDER_EXECUTION=1` gate, `ACP_REQUIRE_AUTH=1`, cost caps, audit trail |
| Rate limit boundary | Requests within configured rate limits | Excessive request rates | `RateLimiter` with configurable window/max, 429 responses |
| Plugin boundary | Registered plugins with valid manifests | Unregistered or malformed plugins | `PluginSystem` validation, thread-safe `RLock` execution |
| SQLite boundary | App-owned local state | External data sources | WAL mode, foreign keys, `PRAGMA integrity_check` |
| CLI executor boundary | Engine process | External CLI tools (`claude`, `codex`) | `spawn_blocking` for async safety, timeout via `ACP_CLI_TIMEOUT_MS` |
| Future supervised-execution boundary | Planned sandbox/workspace/approval/rollback/artifact contracts and Batch 7 patch-workspace plan | Host filesystem, network, target repos, external tools | Slice A metadata only; runtime controls not implemented |

---

## 3. Threats

### T-001: Credential Leakage

**Description:** API keys, provider credentials, or tokens are committed to the repository, logged in events, or exposed in API responses.

**Impact:** Critical — credential compromise enables unauthorized provider access and cost exposure.

**Controls:**
- `check_security_baseline.py` scans for credential patterns in source
- `scripts/acp_secret_scan.py` scans tracked files plus local env files before real local trials
- `redact_secrets()` and `redact_audit_fields()` in provider audit path
- `/api/v1/audit?redact=true` redacts sensitive audit detail keys for operator-facing review
- API keys are hashed; raw keys shown once on creation/rotation
- `.env` is gitignored; `.env.example` documents vars without values

---

### T-002: Unauthorized Provider Call

**Description:** Code makes an outbound HTTP request to a model provider API without proper gating, incurring cost or leaking data.

**Impact:** High — unexpected API calls generate costs and may transmit data to external services.

**Controls:**
- `ACP_ENABLE_PROVIDER_EXECUTION=1` required for real provider types (stub works without it)
- `ACP_REQUIRE_AUTH=1` enforced when provider is active
- `dispatch:execute` scope required for provider dispatches
- Per-dispatch cost cap (`ACP_COST_PER_DISPATCH_USD`) and daily cap (`ACP_COST_DAILY_USD`)
- Provider audit events persist to SQLite for every call
- CI uses stub/mock paths; no real provider calls in automated tests

---

### T-003: API Authentication Bypass

**Description:** Unauthenticated requests access protected endpoints, or revoked/expired keys are accepted.

**Impact:** High — unauthorized access to dispatch, config, team, and backup operations.

**Controls:**
- `TenantResolver` validates key existence, revocation, and expiry
- `last_used_at` tracking on every authenticated request
- Scope-based authorization via `AuthDecision` (403 for missing scopes)
- Rate limiting per key (429 for exceeded limits)
- Admin-only endpoints require `team:admin` scope

---

### T-004: SQLite Data Corruption

**Description:** Concurrent access, crash, or bug corrupts the local SQLite database.

**Impact:** High — loss of dispatch history, config, team state, and audit trail.

**Controls:**
- WAL journal mode for concurrent read/write safety
- `PRAGMA integrity_check` via `check_integrity()` method
- Atomic backup restore with post-restore verification
- Versioned migrations via `PRAGMA user_version`
- Contention tests verify concurrent write safety

---

### T-005: CLI Executor Thread Starvation

**Description:** Synchronous CLI process execution blocks Tokio worker threads, stalling all HTTP handlers.

**Impact:** Medium — API becomes unresponsive during CLI dispatches.

**Controls:**
- Dispatch calls wrapped in `tokio::task::spawn_blocking` in HTTP handler
- CLI timeout via `ACP_CLI_TIMEOUT_MS` environment variable
- Binary detection at startup (graceful degradation if CLI not found)

---

### T-006: Dashboard Silent Failure

**Description:** API errors are silently swallowed, showing misleading "no data" states instead of error messages.

**Impact:** Medium — operator cannot distinguish "empty" from "broken".

**Controls:**
- Structured `ApiError` class with status code awareness
- Visible error states for 401/403/network failures in all dashboard tabs
- `isAuthError()` helper for protected-mode detection

---

### T-007: Backup/Restore Data Loss

**Description:** Backup restore overwrites current state with corrupted or stale data.

**Impact:** High — irreversible data loss.

**Controls:**
- `confirm_restore=true` required for restore endpoint
- `GET /api/v1/backups/:id/verify` checks checksum, SQLite integrity, and table row counts without modifying the live store
- `POST /api/v1/backups/:id/restore/dry-run` reports restore readiness without overwriting the live store
- `scripts/acp_restore_smoke.py` exercises create backup → verify → restore dry-run by default
- `restore_backup_with_verify()` performs post-restore integrity check
- Backup creation requires `backup:admin` scope and `confirm_local_backup=true`
- Admin audit events for all backup operations

---

### T-008: Path Traversal in Checkpoint/Backup

**Description:** User-supplied paths escape the intended directory via `../` or symlink traversal.

**Impact:** High — arbitrary file read/write on host.

**Controls:**
- Path canonicalization and prefix checks in checkpoint manager
- Backup paths constrained to app-owned directory
- No user-supplied paths in backup/restore endpoints (backup IDs only)

---

### T-009: Sandbox Escape In A Future Execution Beta

**Description:** Code or tools run during a future supervised execution beta escape the intended isolation boundary and access host files, network, processes, credentials, or other workflow state.

**Impact:** Critical — host compromise or credential exposure.

**Controls:**
- Not implemented today.
- ADR-0002 Batch 6 requires a selected isolation primitive, resource limits, default-deny network policy, read-only target mount, writable scratch-only policy, audit events, and failure handling before Batch 7 can start.
- ADR-0002 Batch 7 Slice A stores only app-owned workspace/artifact metadata. Slice B exposes only read-only GET metadata views. This is not a process/container/VM sandbox and is acceptable only while target commands, shell execution, package managers, external CLIs, providers, and workers remain forbidden. Any later command execution requires a separate isolation primitive decision.

---

### T-010: Target Workspace Boundary Failure

**Description:** A future execution workspace reads or writes outside its intended scope, mutates a registered target repository directly, or leaks target data through artifacts.

**Impact:** High — unauthorized target mutation or data exfiltration.

**Controls:**
- Not implemented today.
- Current app behavior remains read-only for target repositories.
- ADR-0002 Batch 6 requires an isolated harness-owned workspace, source revision evidence, writable path inventory, final diff/artifact inventory, and no direct target-repo mutation before Batch 7 can start.
- ADR-0002 Batch 7 Slice A rejects registered-target `git worktree add` for metadata records and validates that planned workspace canonical paths are outside registered target repositories. Slice B only exposes this metadata through `dispatch:read` GET routes. It does not create workspace directories or copy target files.

---

### T-011: Approval Bypass In Future Execution

**Description:** A future execution path proceeds without a required human approval, uses stale approval, accepts approval from the wrong identity/scope, or ignores revocation.

**Impact:** Critical — human-gated actions execute without valid authorization.

**Controls:**
- Not implemented today.
- Batch 4 approval records are inert metadata and do not grant execution authority.
- ADR-0002 Batch 6 requires authenticated approver identity, scoped approval authority, decision expiry, revocation behavior, and immutable audit events before Batch 7 can start.
- ADR-0002 Batch 7 Slice A stores patch workspace/artifact metadata that future approval evidence can bind to, and Slice B exposes read-only metadata views, but the patch-review approval gate is not wired.

---

### T-012: Rollback Or Artifact-Capture Failure

**Description:** A future execution failure leaves app state or workspace state partially rolled back, loses evidence, captures secrets without redaction, or stores artifacts in a target repository.

**Impact:** High — inconsistent state, unrecoverable workspace, or sensitive data exposure.

**Controls:**
- Not implemented today.
- ADR-0002 Batch 6 requires all-or-nothing transitions, rollback verification, app-owned artifact storage, redaction before display/export, read-only artifact access, and explicit cleanup rules before Batch 7 can start.
- ADR-0002 Batch 7 Slice A implements minimum app-owned SQLite metadata storage for `supervised_patch_workspace.v1` and `supervised_patch_artifact.v1`, plus normalized changed-file validation and export/import/integrity coverage. Slice B exposes metadata through read-only `dispatch:read` GET routes. Rollback runtime, patch file capture, redaction runtime, access/export gate, and cleanup runtime are not implemented.

---

## 4. Existing Controls

| ID | Control | Addresses |
|----|---------|-----------|
| C-001 | Provider execution gated by `ACP_ENABLE_PROVIDER_EXECUTION=1` | T-002 |
| C-002 | Auth required when provider active (`ACP_REQUIRE_AUTH=1`) | T-002, T-003 |
| C-003 | Per-dispatch and daily cost caps | T-002 |
| C-004 | Provider audit events persisted to SQLite | T-002 |
| C-005 | Secret scanning in CI (`check_security_baseline.py`) | T-001 |
| C-006 | Redaction in audit path (`redact_secrets`, `redact_audit_fields`) | T-001 |
| C-007 | API key scoping and expiry enforcement | T-003 |
| C-008 | Rate limiting with 429 responses | T-003 |
| C-009 | WAL mode + integrity check + versioned migrations | T-004 |
| C-010 | Atomic backup restore with verification | T-007 |
| C-011 | `spawn_blocking` for CLI dispatch in HTTP path | T-005 |
| C-012 | Structured error types in dashboard API client | T-006 |
| C-013 | Path traversal prevention in checkpoint/backup | T-008 |
| C-014 | Thread-safe plugin execution (RLock) | T-003 |
| C-015 | CORS headers on all API responses | T-003 |
| C-016 | Request body size limit on HTTP server | T-003 |
| C-017 | Production-like local ops check (`acp_ops_check.py`) | T-003, T-004, T-006 |
| C-018 | Backup verify and restore dry-run | T-007 |
| C-019 | Local env secret scan (`acp_secret_scan.py`) | T-001 |

## 4.1 Design Gates Not Yet Implemented

| ID | Planning control | Addresses |
|----|------------------|-----------|
| DG-001 | ADR-0002 Batch 6 sandbox/workspace/approval/rollback/artifact contracts | T-009, T-010, T-011, T-012 |
| DG-002 | Batch 7 must receive separate human approval before any supervised execution implementation | T-009, T-010, T-011, T-012 |
| DG-003 | ADR-0002 Batch 7 Slice A/B stores only app-owned patch workspace/artifact metadata, rejects registered-target worktree mutation/path placement, and exposes only read-only metadata views | T-009, T-010, T-011, T-012 |

---

## 5. Residual Risks

### RR-001: No Runtime Secret Scanning

Secret scanning is static (file-level pattern matching). No runtime detection of secrets entering through environment variables or dynamic configuration. **Acceptable** for local-only deployment; would need addressing for any network-exposed deployment.

### RR-002: Single-Host Trust Model

The entire system assumes localhost trust. There is no TLS, no network-level authentication, and no protection against local privilege escalation. **Acceptable** for the documented local-only use case.

### RR-003: No Automated Backup Rotation

Backups accumulate without automatic cleanup. A long-running instance could consume significant disk space. **Mitigated** by manual backup deletion via API/dashboard.

### RR-004: Provider Credential Storage in Environment

Provider API keys live in environment variables, which may be visible in process listings or crash dumps. **Acceptable** for local development; would need a secrets manager for any shared deployment.

### RR-005: No Rate Limit Persistence

Rate limiter state is in-memory. Restart resets rate limits. **Acceptable** for local use; would need persistent storage for production deployment.

### RR-006: No Execution-Phase Controls

Sandbox isolation, target workspace writes, approval broker wiring, rollback engine, and artifact-capture runtime are not implemented. **Acceptable** for the current storage-only Slice A because no execution authority exists. Any approved runtime slice must test controls for T-009 through T-012 before supervised execution beta can be considered.
