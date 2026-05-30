# Threat Model — Local Agent Control Plane

Last updated: 2026-05-30
Scope: Rust engine, TypeScript dashboard/SDK, local SQLite state, env-gated provider adapters

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

---

## 3. Threats

### T-001: Credential Leakage

**Description:** API keys, provider credentials, or tokens are committed to the repository, logged in events, or exposed in API responses.

**Impact:** Critical — credential compromise enables unauthorized provider access and cost exposure.

**Controls:**
- `check_security_baseline.py` scans for credential patterns in source
- `redact_secrets()` and `redact_audit_fields()` in provider audit path
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
