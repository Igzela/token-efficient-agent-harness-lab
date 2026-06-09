# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | Yes                |
| < 0.1   | No                 |

This project is licensed under the MIT License. It is a local research tool, not a production SaaS.

## Reporting a Vulnerability

Report security vulnerabilities through one of these channels:

- **GitHub Security Advisories** (preferred): Use the "Security" tab in the repository to create a private advisory.
- **Email**: Send details to `security@igzela.dev` with subject line starting with `[TEAHL Security]`.

Do **not** open a public issue for security vulnerabilities.

## What to Include

When reporting, please provide:

- Description of the vulnerability and its potential impact
- Steps to reproduce (minimal PoC preferred)
- Affected component (engine, dashboard, SDK, API, or scripts)
- Your assessment of severity (critical / high / medium / low)
- Suggested fix, if any

## Response Timeline

- **Acknowledgment**: Within 48 hours of receiving your report
- **Triage and assessment**: Within 5 business days
- **Fix for critical/high severity**: Within 7 days
- **Fix for medium/low severity**: Next scheduled release

You will be notified when a fix is released and credited in the changelog (unless you prefer otherwise).

## Scope

### In Scope

- Rust engine (`engine/`) — dispatch kernel, storage, API server, authentication
- TypeScript dashboard (`dashboard/`) — UI, API integration
- Python SDK (`sdk/python/`) — client library and auth handling
- API authentication — `TenantResolver`, API key validation, scope enforcement
- Secret management — environment variable handling, credential storage
- Security tooling — `scripts/acp_secret_scan.py`, `tools/check_security_baseline.py`

### Out of Scope

- Third-party model provider APIs and their security posture
- Network deployment configurations beyond localhost (this is a local-only tool)
- User-hosted reverse proxies, TLS termination layers, or firewall rules
- Social engineering attacks against project maintainers
- Denial-of-service against a local single-user instance

## Security Design Highlights

- **Local-only by default**: Engine binds to localhost. No cloud dependencies for core operation.
- **Environment-gated providers**: Real model-provider credentials (`ACP_API_KEY` and related) are opt-in via environment variables. No real API calls are made by default.
- **API key authentication**: Scoped tenant keys with role-based access control. Keys are stored in SQLite and never logged.
- **Audit logging**: Immutable audit trail records all state mutations in the SQLite ledger.
- **Secret scanning**: `scripts/acp_secret_scan.py` enforces no secrets in the repository.
- **Security baseline**: `tools/check_security_baseline.py` validates configuration against known-good defaults.
- **Threat model**: Maintained in `docs/security/THREAT_MODEL.md` with a full asset inventory, trust boundaries, and control matrix.

For deeper detail, see:

- [Threat Model](docs/security/THREAT_MODEL.md)
- [Security Controls Matrix](docs/security/SECURITY_CONTROLS_MATRIX.md)
- [Scope Templates](docs/security/SCOPE_TEMPLATES.md)
