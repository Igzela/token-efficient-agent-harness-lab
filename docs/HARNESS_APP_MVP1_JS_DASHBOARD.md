# Harness App MVP1 — Read-Only JavaScript Dashboard

## Purpose

Harness App MVP1 adds a static JavaScript dashboard for reading a Harness App
MVP0 instance audit report.

The dashboard is an inspection surface only. It does not run audits, call
providers, start sandboxes, mutate the target repository, approve work, or
merge branches.

## Scope

MVP1 includes:

- `web/dashboard/index.html`
- `web/dashboard/app.js`
- `web/dashboard/style.css`
- `web/dashboard/sample_audit_report.json`

The dashboard displays:

- audit verdict
- target repository
- control check statuses and evidence
- warnings
- blockers
- recommended next actions
- fixed boundary reminders

It can render the bundled sample report and can load another local JSON report
through the browser file picker.

## Usage

Generate a report:

```bash
python3 tools/harness_instance_audit.py --target-repo ../alters-lab --json
```

Open:

```text
web/dashboard/index.html
```

Use the `Load JSON` control to inspect a generated report.

## Boundaries

MVP1 is:

- static
- read-only
- local-only
- dependency-free
- no server
- no provider
- no sandbox
- no target repository writes
- no approval automation
- no Stage 5 implementation
- no production deployment

## Relationship to MVP0

MVP0 remains the audit authority for this application layer. MVP1 only displays
the MVP0 JSON shape; it does not duplicate auditor logic in JavaScript.
