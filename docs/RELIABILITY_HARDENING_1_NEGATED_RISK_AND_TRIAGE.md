# Reliability Hardening 1: Negated Risk and Triage Differentiation

## Summary

This hardening addresses the main Trial 1 finding: low-risk read-only tasks were
over-gated when their constraints contained negated risk phrases such as `no
target repo writes`. The deterministic planner treated the keyword `writes` as
positive mutation intent even when the phrase was a safety boundary.

The change keeps real risk gates intact while improving the usefulness of
multi-plan token-budget review.

## Scope

Changed behavior is limited to:

- deterministic risk detection in the resource planner
- review guidance option ordering for budget-pressure plans
- portfolio triage priority and bottleneck differentiation

No API schema, UI, provider, sandbox, worker, or target repository behavior was
added.

## Behavior

Negated boundary statements such as `no target repo writes`, `target repo
remains read-only`, `no source changes`, `audit only`, and `review only` no
longer trigger mutation gates by themselves.

Positive risk statements still trigger gates. Examples include `write target
repo`, `modify target repo`, `commit changes`, `push main`, provider/API-key
work, sandbox/container execution, worker execution, deployment, and high-risk
tasks.

Review guidance can now prioritize budget reduction when a plan's relevant
issue is token budget pressure rather than a true provider, execution, or
deployment gate.

Portfolio triage now differentiates `needs_approval` plans by semantic priority
before falling back to stored order. It considers true gate severity, effective
risk, token pressure, gate count, blockers, and task type.

## Boundary Confirmation

This hardening does not introduce:

- MVP9
- Stage 5
- provider or model calls
- sandbox, process, container, or VM execution
- autonomous workers
- target repository writes
- approval, run, execute, assign, deploy, or merge controls
- persistent event log changes
- schema migrations
- new dependencies
- UI changes
