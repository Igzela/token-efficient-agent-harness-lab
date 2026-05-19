# Dashboard Security and Privacy

## Redaction Rules

The dashboard must redact the following categories of sensitive data:

### 1. No Credentials

API keys, tokens, passwords, and authentication secrets are never displayed. References to credential files may be shown (e.g., "credentials stored in `.env`") but the values are never rendered.

### 2. No Secrets

Secrets included in configuration, environment variables, or secret stores are never displayed. The dashboard may indicate that secrets exist and are configured, but never their contents.

### 3. No Raw Provider Responses

Raw LLM API responses are never displayed in full. The dashboard shows summaries (pass/fail, token counts, cost estimates) but not the raw text of model outputs. This prevents accidental exposure of potentially sensitive information in provider responses.

### 4. No Raw Event Payloads

Event log entries are displayed as summaries (event type, timestamp, brief description) not raw JSON payloads. Full payloads may contain sensitive data.

### 5. No Internal Paths to Sensitive Files

Paths to credential files, secret stores, or sensitive configuration are referenced generically (e.g., "credentials file") not with absolute paths that could expose system structure.

## Display Policy Per Data Type

| Data Type | Display Policy |
|-----------|---------------|
| Gate status | Full display — pass/fail/pending/blocked |
| Gate evidence references | Display file names, not full paths |
| Eval results | Full display — pass/fail, fixture counts |
| Eval raw outputs | Redacted — summaries only |
| Cost estimates | Redacted — ranges or totals only, not exact figures |
| Token usage | Redacted — totals only, not per-request |
| Policy names | Full display |
| Policy content | Full display (policies are not secrets) |
| Approval status | Full display — pending/approved/rejected |
| Approver identity | Display with permission — may be redacted |
| Security check results | Full display — pass/fail/warn |
| Security check raw output | Redacted — summaries only |
| Provider credentials | Never displayed |
| Provider names | Full display |
| Model profiles | Full display |
| Routing rules | Full display |
| Event type | Full display |
| Event timestamp | Full display |
| Event payload | Redacted — summaries only |
| User identity | Display with permission |
| System paths | Redacted — generic references |
| Git commit SHA | Full display |
| Git branch name | Full display |

## Security Checker Relationship

The dashboard reads security checker results from `docs/security/`. The relationship is:

1. **Security checker runs independently.** The dashboard does not trigger security checks.
2. **Dashboard reads results.** The dashboard displays security checker output as a read-only view.
3. **No bypass.** The dashboard cannot suppress, dismiss, or override security warnings.
4. **No escalation.** The dashboard cannot escalate security findings. Escalation remains in the security workflow.
5. **Audit trail.** Security checker results displayed by the dashboard are timestamped and linked to the source check.

## Local-Only First

The first implementation must be local-only:

1. **No network.** The dashboard does not make network requests. No API calls, no external service connections.
2. **No server.** The dashboard runs as a local file (HTML or static report). No HTTP server, no port binding.
3. **No file writes.** The dashboard reads data files but never writes to them.
4. **No process spawning.** The dashboard does not spawn child processes or shell commands.
5. **No browser storage.** If HTML-based, the dashboard does not use localStorage, sessionStorage, IndexedDB, or cookies for persistent state.
6. **No CORS.** Since there are no network requests, CORS is not a concern.

## No Telemetry

The dashboard must not:

1. **Send analytics.** No usage analytics, no page views, no event tracking.
2. **Phone home.** No connections to external servers for any purpose.
3. **Upload data.** No upload of dashboard state, user interactions, or displayed data.
4. **Track users.** No user identification, no session tracking, no fingerprinting.
5. **Log externally.** No external logging services. All logs are local only.

## Data Minimization

The dashboard follows data minimization principles:

1. **Show only what's needed.** Each panel displays only the fields necessary for its purpose.
2. **Aggregate when possible.** Summarize detailed data into high-level metrics.
3. **Redact by default.** When in doubt, redact. Displaying sensitive data is the exception, not the rule.
4. **Time-bound data.** Display data for a defined time window, not unlimited history.
5. **No caching of sensitive data.** Sensitive data is read and displayed, not cached or stored.

## Access Control

Since the dashboard is local-only:

1. **File system permissions.** Access is controlled by file system permissions on the data source files.
2. **No authentication.** The dashboard itself has no login or authentication mechanism.
3. **No authorization.** The dashboard displays all data the user has file system access to.
4. **No multi-user.** The dashboard is designed for single-user local use, not shared access.
