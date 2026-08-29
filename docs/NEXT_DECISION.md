# Next Decision

Last updated: 2026-08-29.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery, PR1 contract freeze, PR2 Shadow Steward acceptance, PR3
provider-free executor acceptance, and PR4A Autonomous Integration Readiness.
PR4A is accepted on main at merge `2e812da126b563665a99a950541f17517b9a4c70`
from PR #640; its exact-head review and canonical PR checks passed, and
post-merge canonical workflow `33210031557` passed all required jobs on that
merge SHA. The current routed window is PR4B in `T3_REQUIRED`: its contract
now binds the observed Vader topology, the actual GitHub controller writer,
the accepted-main Steward service template, Issue #208 owner approval, and a
finite one-forward/one-compensation mutation budget. No effect is executable
until the exact authenticated approval comment is read back after this
contract is accepted. Automatic merge remains disabled in the safe baseline.

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR4B` — `T3_REQUIRED`

**Immediate predecessor bridge:** PR4A is accepted on `main` (PR #640 exact
head `29b4e291d36c21eb5676ce6e47ca08662c095beb`, merge
`2e812da126b563665a99a950541f17517b9a4c70`, exact-head review `PASS`,
canonical PR workflow `33208836187`, post-merge `main` workflow
`33210031557`). It proves provider-free Mission activation and Stage
integration readiness only; the PR4B effect remains behind the exact Issue
#208 approval and the gates below.

## Packet PE7-AUTONOMOUS-STEWARD-PR4B

**State:** `T3_REQUIRED`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR4A` — COMPLETE on accepted main
`2e812da126b563665a99a950541f17517b9a4c70`.

**Class:** `EFFECT`

**Worker tier:** `T3`

**Risk class:** `external_effect`; the effect limit must be finite and
nonzero, freshly authorized, and bound to the named targets and operations.

**Outcome:** After this contract is accepted and the exact Issue #208 owner
approval is read back, run the provider-free canary, prove emergency stop and
rollback, cut over to exactly one lifecycle writer, and enable guarded
maintenance merge only after exact-head review, canonical CI, ruleset, and
recovery gates pass.

**Required prerequisite:** The accepted-main audit observed no system-level
Steward or legacy unit, no `agent-steward` user, no
`/var/lib/agent-steward` journal directory, and no
`/opt/token-efficient-agent-harness-lab` service root. The only old writer is
the GitHub Actions `.github/workflows/agent-controller.yml` command path
through `scripts/agent-control/dispatcher.py` and `state_manager.py`; the
online `actions.runner.Igzela-token-efficient-agent-harness-lab.Vader.service`
is only the runner host and must not be stopped as the writer. The
accepted-main `scripts/agent-control/steward.service` template is disabled by
default and targets `agent-steward`, `/opt/token-efficient-agent-harness-lab`,
and `/var/lib/agent-steward/steward.sqlite3`. The current Issue #208 safe
readback is `emergency_stop=true`, `orchestrator_enabled=false`, and
`auto_merge_enabled=false`.

**Allowed delta:** Only the explicitly named Vader runner, the accepted-main
Steward service template and its minimal service identity/journal
installation, existing GitHub control-state mutations, the bounded
single-writer canary/cutover, guarded maintenance merge, and retained
evidence/rollback; no new controller, queue, ledger, store, evaluator,
workflow owner, or document owner.

Operationally this means provisioning `agent-steward`, provisioning and
permissioning `/var/lib/agent-steward`, and installing the accepted-main
`steward.service` unit without enabling it by default. No repository payload is
copied into `/opt` and no deployment is performed; because the observed
`/opt/token-efficient-agent-harness-lab` root is absent, the unit remains
stopped unless an already-present service root is independently read back.
The `service-start`/`service-stop` budget is therefore conditional and never a
fallback installation path. Read-only reconciliation uses the existing
Steward journal and GitHub facts owners.

Before any writer transition, read the active `agent-controller.yml` runs and
the open `agent-running`/claim state. If an old-controller run is present,
`legacy-controller-stop` may cancel the identified run set once and must read
back a terminal state; if the readback is uncertain, stop and do not retry. If
the set is empty, retain that zero-run receipt and keep the emergency stop
active. Then one `coordinator-activate` operation may run the existing
`scripts/agent-control/steward.py:Steward.execute_stage_to_waiting_for_merge`
path against the approved Mission/Stage; it, not the reconciliation service,
is the named single lifecycle writer. Prove no legacy run, claim, or writer is
active before and during that bounded invocation. No Provider, product target,
release, deployment, credential, or destructive cleanup is included.

**Finite operation ledger:** Each operation identity has one forward attempt
and one compensation attempt maximum; a successful readback ends that
operation, and `OUTCOME_UNKNOWN` is never retried. The complete budget is:

- `service-user-provision`, `journal-directory-provision`, and
  `systemd-unit-install`: one
  forward plus one retained, non-destructive compensation each.
- `service-start`, `service-stop`, `legacy-controller-stop`,
  `coordinator-activate`, `emergency-resume`, `orchestrator-enable`, and
  `guarded-auto-merge-enable`: one forward plus one compensating stop/disable
  action each.

The capsule therefore records `max_forward_mutations=10` and
`max_compensations=10`; no operation may be repeated under a different name.
The default and rollback-safe state is service absent or stopped,
`orchestrator_enabled=false`, `auto_merge_enabled=false`,
`emergency_stop=true`, and the old writer stopped by that emergency gate.

**Approval transport:** The existing `GitHubOwnerApprovalAuthenticator` is the
machine validation owner. After this contract is accepted, regenerate the
proposal/capsule from the new accepted-main SHA, then publish one authenticated
owner comment on Issue #208
containing the exact `steward-owner-approval:v1` marker, the exact capsule
SHA-256 below, `approval_id`, `approved_at`, and accepted-main SHA. Read back
the author, server `createdAt`, issue number, digest, and age before consuming
it. The approval is one-time, replay-protected, and expires 86,400 seconds
after `approved_at`; this contract does not treat the user message itself as
consumed authority.

**Exit:** PR4B closes only after the canary, rollback, one-writer proof,
guarded-merge gates, and retained evidence are accepted.

The closeout evidence must additionally include accepted exact-head review,
canonical CI, active ruleset readback, rollback readiness, canary journal
readback, emergency-stop proof, old-writer absence, new-writer identity,
guarded-maintenance-merge readback, and one real provider-free Mission. It
must record the service/control mutation ledger, any compensation,
read-only reconciliation, and the final safe state.

**Stop:** Stop on uncertain identity, two writers, missing rollback, failed
exact-head/review/CI/ruleset gate, outcome-unknown mutation, forbidden
Provider/target/release/deployment/credential/destructive action, or budget
expansion; never retry an unknown effect. The exact approval and readback are
required before execution or authority consumption.

### PR4B Exact Proposal and T3 Request

This is the reproducible proposal generated from accepted main
`a464bb7b4a399cf9f65fcde6c55e96d076aa3124`. The current-main evidence digest
is `5fecadf806fd176ce4d1300be389f84efe311e7be5b5741ccfdc4a513879d81d`, the
route manifest digest is
`b6e3185023c992cacdb5998d502997adc649b82e9a41efb69911b127bc6d1dbf`, and the
route candidate/spec digest is
`5ee7e9576923c8701c4d1526ffd6582d1a9840939c961d664f4f424b0db5ac24`.
The non-authorizing route capsule remains `external_effect_limit=0`; the
finite effect envelope below is separate and requires the authenticated Issue
approval.

```json
{"approval":{"expires_after_seconds":86400,"issue":208,"marker":"steward-owner-approval:v1","one_time":true,"transport":"authenticated_issue_comment"},"current_main_evidence_sha256":"5fecadf806fd176ce4d1300be389f84efe311e7be5b5741ccfdc4a513879d81d","default_state":{"auto_merge_enabled":false,"emergency_stop":true,"old_writer":"stopped_by_emergency_stop","orchestrator_enabled":false,"service":"absent_or_stopped"},"forbidden":["provider","product_target","release","deployment","credentials","destructive_cleanup","retry_outcome_unknown"],"max_compensations":10,"max_forward_mutations":10,"old_writer":{"controller":"scripts/agent-control/dispatcher.py","runner_is_old_writer":false,"runner_service":"actions.runner.Igzela-token-efficient-agent-harness-lab.Vader.service","state_owner":"scripts/agent-control/state_manager.py","workflow":".github/workflows/agent-controller.yml"},"operation_budget":[{"compensation":1,"forward":1,"operation_id":"service-user-provision"},{"compensation":1,"forward":1,"operation_id":"journal-directory-provision"},{"compensation":1,"forward":1,"operation_id":"systemd-unit-install"},{"compensation":1,"forward":1,"operation_id":"service-start"},{"compensation":1,"forward":1,"operation_id":"service-stop"},{"compensation":1,"forward":1,"operation_id":"legacy-controller-stop"},{"compensation":1,"forward":1,"operation_id":"coordinator-activate"},{"compensation":1,"forward":1,"operation_id":"emergency-resume"},{"compensation":1,"forward":1,"operation_id":"orchestrator-enable"},{"compensation":1,"forward":1,"operation_id":"guarded-auto-merge-enable"}],"packet_id":"PE7-AUTONOMOUS-STEWARD-PR4B","proposal_spec_sha256":"5ee7e9576923c8701c4d1526ffd6582d1a9840939c961d664f4f424b0db5ac24","rollback":"Stop the new service and coordinator, restore emergency stop and disabled controls, retain journal and all receipts; never delete the recovery evidence.","route_manifest_sha256":"b6e3185023c992cacdb5998d502997adc649b82e9a41efb69911b127bc6d1dbf","schema_version":"pr4b_effect_capsule.v1","source_accepted_main_sha":"a464bb7b4a399cf9f65fcde6c55e96d076aa3124","target":{"journal_directory":"/var/lib/agent-steward","repository":"Igzela/token-efficient-agent-harness-lab","runner":"Vader","service_root":"/opt/token-efficient-agent-harness-lab","service_unit":"steward.service","service_user":"agent-steward","template":"scripts/agent-control/steward.service"}}
```

Canonical capsule SHA-256:
`5f6cdfa07c872be0b00cf7d3a156808f1c48fa3ce457069f2977a25ffb1f0cae`.

<!-- route-t3-request:v1
{"accepted_main_sha":"a464bb7b4a399cf9f65fcde6c55e96d076aa3124","action_digest":"02aace3b859c41ad74ef6375aa3ff8aad9f55b1eaeb27ab1c2ec795eabf8f201","authority_owner_digest":"e5836b3304e0e8fe86d135705596e52a0bd0fa9ae50d63d22c007b4693a90934","candidate_digest":"5ee7e9576923c8701c4d1526ffd6582d1a9840939c961d664f4f424b0db5ac24","packet_id":"PE7-AUTONOMOUS-STEWARD-PR4B","requested_action":"After the PR4B contract is accepted, run the separately authorized provider-free canary, prove emergency stop and rollback, cut over to exactly one lifecycle writer, and enable guarded maintenance merge only after exact-head review, canonical CI, ruleset, and recovery gates pass.","scope_digest":"29f7374e77d00a448917646de218bf462e2386a327c61ece4bb12ec0c987ea50","schema_version":"route_t3_request.v1"}
-->

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable contract or
  identity, missing rollback, secret exposure, unknown external mutation,
  second-writer activation, or any service journal crossing into authority.
- Ordinary implementation, test, review, CI, main-drift, tool, and recoverable
  conflict failures remain repairable within their accepted packet; they are
  not wait reasons.
- Never weaken exact-head review, canonical CI, expected-head merge,
  credential, effect, target, release, deployment, recovery, or single-writer
  boundaries.
- Never treat a plan, capsule, branch-local prose, fixture, or worker self-report
  as accepted capability or T3 authority.

## Common Execution Protocol

- `READY_FOR_EXECUTION` and `IN_PROGRESS` are executable packet states only
  when their prerequisites, authority, scope, rollback, and verification are
  current and proved from accepted main; PR4B is a T3 boundary and is not
  ordinary implementation work.
- Ordinary implementation, test, review, CI, main-drift, tool, and recoverable
  conflict failures remain repairable inside an accepted packet. They do not
  authorize skipping PR4B's T3 gate or starting PR5 early.
- A new main, PR head, review receipt, CI result, or canonical-document change
  invalidates stale evidence. GitHub mutations require exact readback, and
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` retains PR5 through PR7 as blocked successors behind the
current PR4B T3 boundary. No later packet may be started until PR4B's effect
closeout is accepted and its own route is refreshed from accepted main.
