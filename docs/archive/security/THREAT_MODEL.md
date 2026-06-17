# Threat Model — Local Agent Control Plane

Last updated: 2026-06-17
Scope: Rust engine, TypeScript dashboard/SDK, local SQLite/PostgreSQL state, env-gated provider adapters, env-gated CLI executors, guarded dashboard controls for app-owned state, supervised execution runtime primitives in app-owned detached workspaces, artifact capture/integrity/approval/export gates, and remaining no-sandbox/no-target-write safety boundaries.

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
| Static dashboard export | Pre-built Next.js UI served by the engine | Medium — can initiate guarded app-owned actions when authenticated |
| App-owned detached workspaces | Temporary supervised patch workspaces outside registered target repos | Critical — command output, copied source, and generated files can contain sensitive data |
| Supervised execution artifacts | Patch artifacts, diffs, evidence manifests, quarantine evidence, and captured files | High — must remain hash-bound, secret-scanned, and export-gated |

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
| Supervised execution boundary | App-owned workspaces, NodeExecutor, artifact capture, approval binding, export gate | Host filesystem, network, target repos, external tools | App-owned detached workspace, command allowlist, no `sh -c`, timeout kill, secret scan, integrity validation, approval/export gate |

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

### T-009: Sandbox Escape In Supervised Execution

**Description:** Code or tools run during supervised execution access host files, network, processes, credentials, or other workflow state beyond the intended app-owned workspace boundary.

**Impact:** Critical — host compromise or credential exposure.

**Controls:**
- No process/container/VM sandbox is implemented; this is a documented residual risk for local-only use.
- Execution remains default-off or explicit: provider calls require `ACP_ENABLE_PROVIDER_EXECUTION=1`, CLI paths require `ACP_ENABLE_CLI_EXECUTION=1`, and workflow ticks require an explicit API/UI action.
- `CommandNodeExecutor` executes allowlisted binaries with direct argv parsing, rejects shell metacharacters, avoids `sh -c`, enforces timeout kill, and records structured stdout/stderr/exit status.
- Supervised work happens in app-owned detached workspaces rather than registered target repositories.
- Artifact capture, integrity validation, approval binding, and export gate prevent unreviewed captured output from being exported.

---

### T-010: Target Workspace Boundary Failure

**Description:** An execution workspace reads or writes outside its intended scope, mutates a registered target repository directly, or leaks target data through artifacts.

**Impact:** High — unauthorized target mutation or data exfiltration.

**Controls:**
- App runtime still does not write registered target repositories.
- Workspace lifecycle creates app-owned detached directories outside registered target paths and records source manifest evidence.
- `capture_patch` diffs the workspace against `.source_manifest.json` and records changed files plus a patch hash.
- Cleanup and quarantine paths keep workspace terminal state explicit.
- Export remains gated by artifact integrity and approval binding checks.

---

### T-011: Approval Bypass In Supervised Execution

**Description:** An execution/export path proceeds without required human approval, uses stale approval, accepts approval for the wrong artifact, or ignores binding expiry.

**Impact:** Critical — human-gated actions execute without valid authorization.

**Controls:**
- Approval binding includes `bound_patch_hash`, `bound_source_revision`, `bound_changed_files`, and `expires_at`.
- Export requires both valid artifact integrity and valid approval binding.
- Wrong patch hash, wrong source revision, wrong changed-file set, expired binding, rejected approval, or stale artifact state blocks export.
- Approval/export actions operate on app-owned artifact records and emit app-owned events/audit evidence.

---

### T-012: Rollback Or Artifact-Capture Failure

**Description:** An execution failure leaves app state or workspace state partially rolled back, loses evidence, captures secrets without redaction, or stores artifacts in a target repository.

**Impact:** High — inconsistent state, unrecoverable workspace, or sensitive data exposure.

**Controls:**
- Workspace cleanup and quarantine are explicit lifecycle operations.
- `capture_patch` stores app-owned artifact metadata, patch hash, changed files, and secret-scan status.
- `validate_artifact_integrity` checks hash/files/workspace/redaction state before export.
- Export gate requires valid approval binding plus artifact integrity.
- No automatic target-repository apply, merge, deploy, or release path exists.

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
| C-020 | CommandNodeExecutor allowlist, metachar rejection, no `sh -c`, timeout kill | T-005, T-009 |
| C-021 | App-owned detached workspace lifecycle | T-009, T-010, T-012 |
| C-022 | Artifact capture secret scan and integrity validation | T-010, T-012 |
| C-023 | Approval binding and export gate | T-011, T-012 |

## 4.1 Remaining Design Gates

| ID | Planning control | Addresses |
|----|------------------|-----------|
| DG-001 | Any process/container/VM sandbox implementation requires separate approval, tests, docs, and rollout plan | T-009 |
| DG-002 | Any target-repository write/apply/merge/deploy control requires separate approval and a new threat-model update | T-010, T-012 |
| DG-003 | Any hosted/cloud/multi-tenant deployment requires tenant isolation, network policy, resource quotas, secrets management, and operational runbooks | T-001, T-002, T-003, T-009 |
| DG-004 | Any unattended autonomous worker loop requires separate approval, scheduler safety gates, audit, rollback, and kill-switch controls | T-009, T-011, T-012 |

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

### RR-006: No Hard Sandbox Isolation

Supervised execution has app-owned workspace, command, artifact, approval, and export controls, but it does not provide process/container/VM sandbox isolation. **Acceptable** only for local, explicit, supervised use with target repositories protected from direct app writes. Hosted/cloud, multi-tenant, unattended-worker, or target-write expansion requires a new approved track.
