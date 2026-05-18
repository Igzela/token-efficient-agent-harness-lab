# v1.2 Addendum — Authority Boundaries and Kernel Classification

Status: canonical addendum to `harness_architecture_book_v1.2_post_closeout_update.md`.

Architecture tags: `orchestrator-kernel`, `post-closeout`, `stage0-4-complete`, `controlled-adaptive-extensions`.

## 1. Purpose

This addendum pins two decisions that were implicit in the Stage 0–4 implementation but should remain explicit for future maintenance:

1. the authority boundary between goal, state, quality, admission, memory, and orchestration;
2. the kernel classification of this repository.

This is a documentation-only update. It does not change runtime behavior.

## 2. Authority Boundary

The system does not allow a single agent, model, or orchestrator to reinterpret the global objective by itself.

Canonical responsibility split:

```text
Project Brief = goal source
Project Board = state source of truth
Quality Gate = quality / risk evaluator
Final Gate = highest admission function
Memory / Optimization Plane = experience sedimentation layer
Orchestrator = deterministic coordination and state progression controller
```

Rules:

```text
Project Brief defines objective, constraints, success_criteria, and non_goals.
Project Board records factual item state; it does not reinterpret the objective.
Quality Gate evaluates score, artifact state, trajectory, risk, and human-review needs.
Final Gate is the only component allowed to move a project item from review to done.
Memory / Optimization Plane stores run logs, eval records, baselines, skills, policy candidates, and feedback.
Orchestrator does not own goals, does not own long-term memory, and does not directly mutate strategy.
```

If Final Gate conflicts with Project Brief or Quality Gate, the correct behavior is not objective rewriting by the Orchestrator. The correct behavior is:

```text
Final Gate returns fail / requires_human_review.
Project Board moves to review / blocked / failed as appropriate.
Human Owner updates the Project Brief or approves a separate follow-up track.
Memory Plane records evidence and retrospective context.
```

Therefore:

```text
Highest execution admission point: Final Gate.
Highest goal source: Project Brief.
Highest state source: Project Board.
Highest quality evaluator: Quality Gate.
Memory owner: Memory / Optimization Plane.
```

## 3. Kernel Classification

The current system is best classified as:

```text
Orchestrator Kernel
with controlled adaptive-cognitive extensions
```

It is not yet a full Adaptive Cognitive Kernel.

Reasons:

```text
1. Kernel / Orchestrator is deterministic Python control logic, not a model brain.
2. It advances state through EventStore, ProjectionStore, Kernel, BatchRunner, TaskRecordStore, and FinalGateRunner.
3. Intelligence is externalized into controlled components: Advisor Broker, Model Gateway Stub, Routing Experiment Manager, Sampling Runner, and Skill Extractor.
4. Memory belongs to Memory / Optimization Plane, not the Orchestrator internals.
5. Strategy updates, skill extraction, feedback, Keep Rate, and routing experiments must pass through evaluation and human-approved tracks.
6. The repository currently has no real model calls, real autonomous agents, autonomous goal rewriting, or autonomous long-term memory writes.
```

More precise description:

```text
An event-driven, replayable, auditable Orchestrator Kernel surrounded by controlled advisor, quality, memory, and future adaptive-cognitive extension points.
```

## 4. What Would Be Required to Move Toward Adaptive Cognitive Kernel

The repository should not claim full Adaptive Cognitive Kernel status until these tracks are implemented, reviewed, and accepted:

```text
Harness Change Evaluation
Tool/Error Taxonomy Hardening
Outcome Feedback and Maintenance Loop
Context Pack v2
Model Harness Profile
Specialist Agent Role Profiles
Advisor-only Real Model Test
```

Even after these are complete, any adaptive behavior must remain gated by:

```text
fixed evaluation suites
human approval for policy adoption
no automatic goal rewriting
no uncontrolled prompt mutation
no automatic deployment of routing policy
clear rollback path
```

## 5. Design Decisions

### D1. Final Gate is the highest admission function, not the goal source

Project Brief defines the goal. Final Gate decides whether a completed task may count as project-level done.

### D2. Orchestrator does not own memory

Orchestrator may trigger writes to EventStore and pass evidence_refs, but durable memory belongs to Memory / Optimization Plane.

### D3. Current kernel type is Orchestrator Kernel

The implementation is a deterministic coordination kernel with controlled adaptive extension points. It is not yet a self-adapting cognitive kernel.
