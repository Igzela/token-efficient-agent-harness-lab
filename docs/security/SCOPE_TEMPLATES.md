# Local API Scope Templates

These templates are for the local Agent Control Plane API when `ACP_REQUIRE_AUTH=1`.
They are least-privilege starting points for app-owned local state. They do not
grant target-repository writes, deployment authority, sandbox execution, provider
failover, or cloud production access.

## Available Scopes

| Scope | Grants |
|---|---|
| `health:read` | Health, readiness, provider health, metrics, and storage integrity checks. |
| `dispatch:read` | Dispatch history/detail reads. |
| `dispatch:execute` | Submit dispatch requests. Required for real-provider dispatch when provider execution is explicitly enabled. |
| `audit:read` | Audit and provider-audit reads. Use `redact=true` for operator-facing views. |
| `cost:read` | Cost summary and per-dispatch cost reads. |
| `config:read` | Local runtime config reads. |
| `config:admin` | Import app-owned local state with explicit confirmation. |
| `export:read` | Export app-owned local state. |
| `backup:admin` | List, create, verify, delete, dry-run restore, and confirmed restore of local SQLite backups. |
| `team:read` | Team and API-key metadata reads. Raw keys are never returned after creation. |
| `team:admin` | Create/revoke/rotate/delete keys, update key scopes, and manage team members. |

## Templates

| Template | Scopes | Use |
|---|---|---|
| `ops-readonly` | `health:read`, `audit:read` | Dashboard Operations page, `/api/v1/metrics`, storage integrity, provider health, and redacted audit review. |
| `cost-auditor` | `health:read`, `cost:read`, `audit:read` | Cost and provider usage review without dispatch or backup authority. |
| `dispatch-operator` | `health:read`, `dispatch:read`, `dispatch:execute`, `cost:read`, `audit:read` | Real local task trials with budget/cost visibility. |
| `backup-operator` | `health:read`, `backup:admin`, `audit:read` | Backup list/create/verify/delete and restore dry-run. Real restore still requires `confirm_restore=true`. |
| `team-admin` | `health:read`, `team:read`, `team:admin`, `audit:read` | Local user/key administration only. Pair with another key for dispatch or backup work. |
| `local-admin-break-glass` | all available scopes | One local machine owner key for setup and emergency recovery. Keep unshared, rotate after use, and do not commit. |

## Suggested Trial Setup

Use separate keys for real local testing:

- One `dispatch-operator` key for model-task trials.
- One `backup-operator` key for backup/restore smoke.
- One `ops-readonly` or `cost-auditor` key for dashboard review.
- Keep the `local-admin-break-glass` key offline or in a local secret manager.

Provider credentials are separate from API scopes. `ACP_API_KEY` names the local
environment variable that contains the provider secret; the API key used in the
`Authorization` header only authorizes Agent Control Plane endpoints.
