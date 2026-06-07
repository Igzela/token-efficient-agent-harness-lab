export const meta = {
  name: 'sg5-ga-runbook-drill',
  description: 'SG-5 GA Release/Runbook Drill: add runbook doc, rollback drill script, release checklist, and update all handoff surfaces',
  phases: [
    { title: 'Runbook', detail: 'Create the self-hosted GA runbook document' },
    { title: 'RollbackDrill', detail: 'Create the rollback drill script' },
    { title: 'ReleaseChecklist', detail: 'Create the release checklist script' },
    { title: 'HandoffDocs', detail: 'Update NEXT_DECISION, CURRENT_STATUS, MODULE_MAP, README, CLAUDE, AGENTS' },
    { title: 'Verify', detail: 'Run handoff guard and check all scripts' },
  ],
};

// SG-5: GA Release/Runbook Drill
// The existing scripts already cover most operational needs:
//   - start_production_like_local.sh (startup)
//   - acp_local_doctor.py (config validation / readiness)
//   - acp_ops_check.py (operational checks)
//   - acp_restore_smoke.py (backup verify + restore dry-run)
//   - acp_secret_scan.py (secret scan)
//   - soak_ops_drill.py (soak / failure injection)
//   - smoke_release.sh (release tarball smoke)
//   - package-release.sh, install.sh, upgrade.sh (packaging / upgrade)
//
// What's missing for handoff-readiness:
//   1. A consolidated runbook document that ties all scripts together with
//      clear operator procedures for startup, config, upgrade, backup, restore,
//      incident triage, secret scan, and rollback.
//   2. A rollback drill script that exercises the full upgrade → verify → rollback flow.
//   3. A release checklist script that validates all pre-release gates.
//   4. Updated handoff docs reflecting SG-5 completion.

// ── Phase 1: Create the self-hosted GA runbook ──
phase('Runbook');
await agent(
  `Create a comprehensive self-hosted GA runbook at docs/RUNBOOK.md.

The runbook must cover these sections with step-by-step operator procedures:

## 1. Prerequisites
- Toolchain: Rust stable, Bun 22+, uv, Node 22
- Verify: "uv run --no-project python scripts/acp_local_doctor.py"

## 2. First-Time Setup
- Clone repo
- "uv run --no-project python scripts/bootstrap_local_auth.py --json" to generate admin key
- Copy .env.production-like.local.example → .env.production-like.local
- Fill ACP_ADMIN_API_KEY, export ACP_CN_ANTHROPIC_API_KEY if using provider
- Build dashboard: "cd dashboard && bun install --frozen-lockfile && bun run build:static"
- Start: "scripts/start_production_like_local.sh"
- Verify: "curl http://127.0.0.1:8080/api/v1/health"

## 3. Daily Operations
- Health check: "uv run --no-project python scripts/acp_ops_check.py --token \$ACP_ADMIN_API_KEY"
- Metrics: "curl http://127.0.0.1:8080/api/v1/metrics"
- Dashboard: open http://127.0.0.1:8080

## 4. Backup and Restore
- Create backup: POST /api/v1/backups with confirm_local_backup=true
- Verify backup: GET /api/v1/backups/{id}/verify
- Restore dry-run: POST /api/v1/backups/{id}/restore/dry-run
- Full restore smoke: "uv run --no-project python scripts/acp_restore_smoke.py --token \$ACP_ADMIN_API_KEY"

## 5. Upgrade Flow
- Run release drill: "uv run --no-project python scripts/ga_release_checklist.py --token \$ACP_ADMIN_API_KEY"
- Build release: "bash scripts/package-release.sh"
- Upgrade: "bash scripts/upgrade.sh --prefix /usr/local/bin"
- Smoke: "bash scripts/smoke_release.sh <tarball>"

## 6. Rollback Drill
- Run: "uv run --no-project python scripts/ga_rollback_drill.py --token \$ACP_ADMIN_API_KEY"
- Steps: create backup → upgrade simulation → verify → rollback → verify

## 7. Incident Triage
- Check health: "uv run --no-project python scripts/acp_ops_check.py --token \$ACP_ADMIN_API_KEY"
- Check integrity: GET /api/v1/storage/integrity
- Check audit: GET /api/v1/audit?limit=50
- Check scheduler: GET /api/v1/scheduler/status
- Check executor pool: GET /api/v1/executor-pool
- Check decisions: GET /api/v1/decisions?limit=20
- Check queue: GET /api/v1/queue/status
- Check metrics: GET /api/v1/metrics

## 8. Secret Scan
- Run: "uv run --no-project python scripts/acp_secret_scan.py"
- Before provider trials, always scan first.

## 9. Configuration Reference
- Reference .env.example for all available env vars
- Key env vars: ACP_PROFILE, ACP_REQUIRE_AUTH, ACP_ADMIN_API_KEY, ACP_DB_PATH, ACP_BACKUP_DIR, ACP_DASHBOARD_DIR, ACP_PROVIDER_TYPE, ACP_ENABLE_PROVIDER_EXECUTION, ACP_ENABLE_CLI_EXECUTION, ACP_SCHEDULER_EXECUTOR, ACP_CORS_ORIGINS

## 10. Release Checklist
- Run: "uv run --no-project python scripts/ga_release_checklist.py --token \$ACP_ADMIN_API_KEY"
- Checks: secret scan, ops health, backup verify, restore dry-run, storage integrity, dashboard build

Write the file with clear markdown formatting. Use code blocks for all commands. Keep it practical and operator-focused.`,
  { label: 'runbook-author', phase: 'Runbook', model: 'opus' }
);

// ── Phase 2: Create rollback drill script ──
phase('RollbackDrill');
await agent(
  `Create scripts/ga_rollback_drill.py — a stdlib-only Python script (no external deps) that exercises a non-destructive rollback drill against a running ACP API.

The script must:
1. Accept --base-url (default http://127.0.0.1:8080) and --token (admin API key)
2. Accept --json for machine-readable output
3. Step 1: Health check (GET /api/v1/health) — must return 200
4. Step 2: Create backup (POST /api/v1/backups with confirm_local_backup=true and label "rollback-drill")
5. Step 3: Verify backup (GET /api/v1/backups/{backup_id}/verify)
6. Step 4: Restore dry-run (POST /api/v1/backups/{backup_id}/restore/dry-run with confirm_restore_dry_run=true)
7. Step 5: Storage integrity check (GET /api/v1/storage/integrity)
8. Step 6: Metrics snapshot (GET /api/v1/metrics) — record dispatch_count, audit_count, backup_count
9. Step 7: Second health check to confirm stability
10. Print a summary with pass/fail per step and overall verdict
11. Exit 0 if all pass, exit 1 if any fail
12. Support --json flag for JSON output

Follow the style of existing scripts like acp_ops_check.py and acp_restore_smoke.py:
- Use urllib.request (stdlib only)
- Use argparse
- Print step results with ✓/✗ markers
- Return structured results

Read scripts/acp_ops_check.py and scripts/acp_restore_smoke.py first to match their patterns exactly.`,
  { label: 'rollback-drill', phase: 'RollbackDrill', model: 'opus' }
);

// ── Phase 3: Create release checklist script ──
phase('ReleaseChecklist');
await agent(
  `Create scripts/ga_release_checklist.py — a stdlib-only Python script (no external deps) that validates all pre-release gates against a running ACP API.

The script must:
1. Accept --base-url (default http://127.0.0.1:8080) and --token (admin API key)
2. Accept --json for machine-readable output
3. Check 1: Secret scan — run "uv run --no-project python scripts/acp_secret_scan.py" as subprocess, check exit code
4. Check 2: Health + readiness (GET /api/v1/health, GET /api/v1/ready)
5. Check 3: Storage integrity (GET /api/v1/storage/integrity)
6. Check 4: Backup creation + verify (POST /api/v1/backups, GET verify)
7. Check 5: Restore dry-run (POST restore/dry-run)
8. Check 6: Metrics health (GET /api/v1/metrics — verify dispatch_count >= 0, audit_count >= 0)
9. Check 7: Dashboard build check — verify dashboard/out/index.html exists
10. Check 8: Config validation — verify ACP_REQUIRE_AUTH=1 is set in the running instance
11. Print a checklist summary with pass/fail/warn per check
12. Print overall verdict: READY or NOT READY
13. Exit 0 if READY, exit 1 if NOT READY
14. Support --json flag for JSON output

Follow the style of scripts/acp_ops_check.py. Read it first to match patterns.

The script should be practical: a release manager runs this before cutting a tarball.`,
  { label: 'release-checklist', phase: 'ReleaseChecklist', model: 'opus' }
);

// ── Phase 4: Update handoff docs ──
phase('HandoffDocs');
await agent(
  `Update the following 6 handoff documents to reflect SG-5 completion. Read each file first, then make the smallest necessary edits.

1. docs/NEXT_DECISION.md — In the Self-Hosted GA Readiness Track table, mark SG-5 as DONE with this text:
   "**DONE** — docs/RUNBOOK.md covers startup, config, upgrade, backup, restore dry-run, incident triage, secret scan, rollback drill, and release checklist. scripts/ga_release_checklist.py validates all pre-release gates. scripts/ga_rollback_drill.py exercises backup→verify→restore-dry-run→integrity→metrics flow."
   Also update the "Next safe action" line at the bottom to say: "SG-1 through SG-5 are complete. The Self-Hosted GA Readiness Track is done. Maintain repo health until the user provides new direction."

2. docs/CURRENT_STATUS.md — In the Macro-Orchestrator Readiness table, update the "Small-team self-hosted GA" row: change "SG-1 through SG-4 complete; SG-5 next" to "SG-1 through SG-5 COMPLETE; track done". In the "Current product gap" paragraph, update to say the self-hosted GA readiness track is complete. In the Self-Hosted GA Readiness table, mark SG-5 as DONE.

3. docs/MODULE_MAP.md — In the Self-Hosted GA Ownership Map table, mark SG-5 as DONE with the runbook, release checklist, and rollback drill script references.

4. README.md — In the "Next Recommended Work" section, update to say "Self-Hosted GA Readiness Track SG-1 through SG-5 is complete." and that the next safe work is maintaining repo health.

5. CLAUDE.md — In the "Current State" section, update the SG-5 line from "NEXT" to "COMPLETE". Update the test count if it changed (should still be 1348).

6. AGENTS.md — In the "Current Status" section, update to reflect SG-5 completion.

IMPORTANT: Read each file first. Make minimal, targeted edits. Do NOT rewrite entire files. Only change the lines that need updating.`,
  { label: 'handoff-update', phase: 'HandoffDocs', model: 'opus' }
);

// ── Phase 5: Verify ──
phase('Verify');
const results = await parallel([
  () => agent(
    `Run the handoff guard and check for documentation drift:
1. Run: uv run --no-project python scripts/check_agent_handoff.py
2. Run: bash scripts/check_wire_codegen_drift.sh
3. Run: git diff --check
4. Verify scripts/ga_rollback_drill.py --help exits 0
5. Verify scripts/ga_release_checklist.py --help exits 0

Report all results. If any fail, explain what went wrong.`,
    { label: 'verify-handoff', phase: 'Verify', model: 'sonnet' }
  ),
  () => agent(
    `Run the secret scan and security baseline to make sure the new scripts don't introduce issues:
1. Run: uv run --no-project python scripts/acp_secret_scan.py
2. Run: uv run --no-project python tools/check_security_baseline.py
3. Verify no new files contain hardcoded secrets or credentials

Report all results.`,
    { label: 'verify-security', phase: 'Verify', model: 'sonnet' }
  ),
]);

return { verification: results };
