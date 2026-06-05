# Security Review

This directory contains the CA-7 sealed-baseline security review plus the
current local Agent Control Plane threat model.

## What This Is

A structured security record for the local harness. `CA7_SECURITY_REVIEW.md`
preserves the sealed-baseline assessment at commit `aedcc81`. `THREAT_MODEL.md`
tracks the current Rust/TypeScript local Agent Control Plane and includes
Batch 6 supervised-execution design-gate risks as non-implemented planning
items.

## What This Is Not

- **This is not a production security certification.** The sealed baseline
  operates without real model providers, without deployed credentials, and
  without external network access.
- **This does not approve CA-8, sandbox execution, target-repo writes, real
  workers, or productionization.** Each of those milestones requires a separate,
  scope-appropriate security review and human approval.
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
| `THREAT_MODEL.md` | Current local Agent Control Plane assets, trust boundaries, threats, controls, residual risks, and Batch 6 design-gate risks |
| `SECURITY_CONTROLS_MATRIX.md` | Traceable control IDs with evidence and test coverage |
| `CA7_SECURITY_REVIEW.md` | Executive summary, findings, and recommendations |
| `SCOPE_TEMPLATES.md` | Local API least-privilege scope templates |
