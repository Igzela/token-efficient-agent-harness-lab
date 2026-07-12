---
name: Agent Task
about: Define an executable task for the autonomous coding agent
title: ''
labels: agent-draft
assignees: ''
---

## Goal

<!-- One observable result. What should be true after this task is complete? -->

## Scope

### Allowed Changes

<!-- Minimum coherent implementation surface. -->

### Forbidden Changes

<!--
- No parallel runtime, scheduler, store, policy authority, or Dashboard state model
- No provision of real secrets, test/CI evidence falsification, or rollback removal
- No merge/deploy/release authority changes outside scope
-->

## Acceptance Criteria

<!-- Measurable gates the implementation must pass. -->

- [ ] Focused tests pass
- [ ] Existing tests remain green
- [ ] Lint and format pass
- [ ] No security baseline regression
- [ ] Agent handoff guard passes
- [ ] All required CI jobs green

## Dependencies

<!-- Issues that must be complete before this one starts. Use "Depends on #N" syntax. -->

## Contracts

<!-- Input/output contracts, versioned schemas, reason codes, bounds, and permissions. -->

## Authority

<!-- What authority does this task need? What must it NOT do? -->

## Failure States

<!-- What should happen if the task cannot be completed? -->

## Verification

```bash
# Commands to verify the implementation
```

## Rollback

<!-- How to revert if needed. -->
