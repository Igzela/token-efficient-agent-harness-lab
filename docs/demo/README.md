# Harness App Demo Package

This demo package is for the **local deterministic non-executable Harness App** (MVP0–MVP8).

## What This Is

A repeatable walkthrough of the Harness App running locally. The app provides a read-only control plane for registering target repositories, auditing them, generating non-executable resource plans, and inspecting review guidance, portfolio triage, and operations diagnostics.

## What This Is Not

- **Not CA-8.** The CA-7 sealed baseline is complete. This demo does not extend it.
- **Not Stage 5.** No Stage 5 implementation exists. This demo does not start one.
- **Not a production runtime.** No real model providers, sandboxes, workers, or deployment targets are involved.
- **Not an execution engine.** Plans are non-executable. The app does not run, approve, assign, or deploy anything.

## Prerequisites

- Python 3.10+
- Node.js (for dashboard syntax check only)
- A local target repository with harness control files (e.g., `alters-lab`)
- This repository checked out at the `main` branch (MVP0–MVP8 complete)

## Recommended Reading Order

1. **[QUICKSTART.md](QUICKSTART.md)** — Start the app and confirm it works.
2. **[DEMO_SCRIPT.md](DEMO_SCRIPT.md)** — Step-by-step operator walkthrough.
3. **[EXPECTED_RESULTS.md](EXPECTED_RESULTS.md)** — What each step should produce.
4. **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** — Common issues and safe fixes.
5. **[BOUNDARIES.md](BOUNDARIES.md)** — What the app may and may not do.

## Scope

This demo covers:

- Starting the local app server
- Registering a local target repository
- Running a read-only audit
- Creating non-executable resource plans
- Inspecting plan review guidance
- Inspecting portfolio triage
- Inspecting operations diagnostics
- Verifying the target repository was not modified
- Stopping the server safely
