# Security Review — CA-7 Sealed Baseline

This directory contains the security review artifacts produced against the CA-7
sealed baseline of the Token-Efficient Agent Harness Lab.

## What This Is

A structured assessment of the codebase at the CA-7 sealed baseline commit
(`aedcc81`). It identifies assets, trust boundaries, threats, existing controls,
and residual risks **as they exist today** in the stage-0 / sealed-baseline
configuration.

## What This Is Not

- **This is not a production security certification.** The sealed baseline
  operates without real model providers, without deployed credentials, and
  without external network access.
- **This does not cover CA-8, real provider integration, sandbox execution,
  or productionization.** Each of those milestones requires a separate,
  scope-appropriate security review.
- **This does not replace a formal threat model or penetration test.** It is a
  baseline reference for future security work.

## How to Use This

These documents serve as the **starting point** for:

- CA-8 security review (real provider integration)
- Provider onboarding security checks
- Sandbox design and escape-threat analysis
- Production readiness reviews
- Human-approval gate design

When a future milestone changes the trust model (e.g., enabling real API calls,
introducing credentials, or allowing sandbox execution), revisit this review
and extend it to cover the new attack surface.

## Files

| File | Purpose |
|------|---------|
| `README.md` | This file |
| `THREAT_MODEL.md` | Assets, trust boundaries, threats, controls, residual risks |
| `SECURITY_CONTROLS_MATRIX.md` | Traceable control IDs with evidence and test coverage |
| `CA7_SECURITY_REVIEW.md` | Executive summary, findings, and recommendations |
| `SCOPE_TEMPLATES.md` | Local API least-privilege scope templates |
