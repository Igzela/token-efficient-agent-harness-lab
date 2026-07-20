---
name: Agent Task (maintainers)
description: Maintainer-only executable task for the repository agent orchestrator
title: ""
labels: ["agent-draft"]
assignees: []
---

> **Maintainer / orchestrator only.** External contributors should use Bug report, Feature request, or External validation.

## Goal

<!-- One observable result. What should be true after this task is complete? -->

## Scope

### Allowed Changes

<!-- Minimum coherent implementation surface. -->

### Machine-readable scope

Edit this marker before applying `agent-ready`. Use explicit files or narrow directory prefixes; do not use `.` or wildcard paths.

<!-- agent-orchestrator-scope:v1
{"allowed_paths":["src/","tests/"]}
-->

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
- [ ] Handoff / docs updated if behavior or contracts change

## Risk and rollback

<!-- Risk class and exact rollback. -->
