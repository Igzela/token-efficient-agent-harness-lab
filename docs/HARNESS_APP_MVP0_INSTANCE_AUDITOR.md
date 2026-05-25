# Harness App MVP0 — Read-Only Project Instance Auditor

## Purpose

Harness App MVP0 is the first executable application shell around the CA-7 sealed baseline. It audits a target repository that claims to use the Token-Efficient Agent Harness as a project governance layer.

The auditor is intentionally read-only. It does not execute tasks, call providers, start sandboxes, mutate the target repository, or approve work. Its job is to answer one narrow question:

> Does this project instance have enough harness control structure to continue, or should it stop for governance repair?

## Scope

The MVP0 auditor checks a target repository for:

- required harness control files
- AGENTS.md execution-adapter policy
- project board sanity
- task queue sanity
- quality gate coverage
- risk register coverage
- closeout report evidence

It returns one of three verdicts:

- `PASS` — enough controls and no obvious governance blockers
- `PASS_WITH_NOTES` — usable, but warnings should be reviewed
- `BLOCKED` — required governance files or hard boundaries are missing or unsafe

## Non-Goals

MVP0 does not:

- start CA-8
- call real models or providers
- read credentials
- execute sandbox/process/container workloads
- modify the target project
- create PRs
- merge branches
- activate policy
- replace human approval

## Usage

Human-readable report:

```bash
python tools/harness_instance_audit.py --target-repo ../alters-lab
```

JSON report:

```bash
python tools/harness_instance_audit.py --target-repo ../alters-lab --json
```

The command exits with code `2` only when the verdict is `BLOCKED`. `PASS` and `PASS_WITH_NOTES` exit with `0` so the report can be used in exploratory audits without failing CI on warnings.

## Expected First Real Target

The first intended real target is `alters-lab`.

Expected initial verdict: `PASS_WITH_NOTES`.

Reasoning:

- It has strong harness control files and phase closeout reports.
- It uses Claude Code as an execution adapter rather than governance authority.
- It tracks project board, task queue, quality gates, decisions, risks, and closeouts.
- It remains heavily manual and would benefit from machine-readable evidence/status indexes.
- Broad automation policy should remain warning-level only if pause conditions are explicit.

## Relationship to CA-7 Sealed Baseline

This tool does not change CA gate definitions. It is an application-layer helper for applying the CA-7 sealed baseline to real project instances.

The kernel remains the authority. The auditor only reads a target project and emits a report.

## Boundary Confirmation

Harness App MVP0 is:

- read-only
- stdlib-only
- local-only
- no-provider
- no-sandbox
- no-UI
- no-policy-activation

Any future move from audit to execution control should be a separate approved track, likely MVP1: Execution Slice Controller.
