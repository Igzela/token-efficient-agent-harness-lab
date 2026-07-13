# Next Decision

## Current Direction

The dispatch kernel, V2 output authority, Adaptive Fusion through AF-7, Agent Runtime through AR-6, Trusted Local Autonomous Execution through IAE-3, and PE-1 through PE-4 are complete and acceptance-sealed.

PE-4 is sealed under `PE4-POST-CLOSE-REPAIR-1`. Its final accepted evidence is:

- PR #206
- exact final head `80d9f9342956e1fd5931b59dcc426908d450b32b`
- merge commit `f2a736a39e5de82d60da2a0b64d1c255d55ec326`
- exact-final-head CI run `29190482093`, seven of seven required jobs passed
- post-merge `main` CI run `29190797214`, seven of seven required jobs passed

Older PE-4 replay contracts, artifacts, and closeout claims remain historical only and cannot authorize current replay, shadow, canary, or promotion behavior.

The active direction is now:

1. complete PE-5 Release Provenance;
2. independently acceptance-seal PE-5;
3. complete PE-6 Fault Injection and Recovery Drills;
4. independently acceptance-seal PE-6.

Do not create another roadmap or product-evolution document. This file is the normative forward plan.

## Execution Protocol

Use one coherent branch and PR per packet unless a bounded prerequisite repair must be separated to preserve reviewability or rollback.

Packet states:

- `READY_FOR_EXECUTION` — prerequisites and contract are sufficient to begin;
- `BLOCKED_PREREQUISITE` — defined but waiting for an earlier packet;
- `DECISION_REQUIRED` — a material decision is unresolved and cannot be safely derived from repository evidence;
- `IN_PROGRESS` — one active branch or PR owns the packet;
- `COMPLETE` — implementation and acceptance evidence are merged and active documents are synchronized.

Every implementation packet must state and verify:

- exact goal and observable result;
- prerequisites and owning paths;
- allowed and forbidden changes;
- versioned inputs, outputs, reason codes, bounds, and failure states;
- authority, permissions, credentials, audit, compensation, and rollback where applicable;
- compatibility with existing release artifacts, SQLite/PostgreSQL data, APIs/SDKs, installers, workflows, and old callers where applicable;
- focused tests and full applicable repository verification;
- exact final-head CI before merge and post-merge `main` CI;
- residual risk and next packet state.

Strict documentation-only corrections may use the targeted merge gate in `docs/REAL_WORLD_TESTING_PLAYBOOK.md`. Any code, script, workflow, configuration, dependency, schema, migration, generated artifact, executable, release, or external-state change requires the complete applicable CI matrix.

## Hard Stops

Stop and report exact evidence rather than improvising when:

- a real secret, private signing key, recovery credential, or unredacted sensitive payload would enter version control or an artifact;
- required test or CI evidence would be falsified or a known failure hidden;
- an irreversible external action lacks explicit authority and a tested recovery path;
- required human approval, external credentials, signing identity, release authority, or protected environment is unavailable;
- another active PR owns conflicting implementation work that cannot be safely rebased or separated;
- an existing authority or rollback owner would be bypassed or silently replaced;
- a destructive fault cannot be bounded to disposable local/CI resources;
- required exact-head CI is failed, queued, in progress, cancelled, action-required, or unexpectedly skipped.

A stale document, an initial failed implementation, or a bounded missing design detail is not itself a hard stop. Audit, repair, test, and continue when the result remains explicit, compatible, observable, and rollbackable.

## Coordination with PR #207

PR #207 is a separate disabled-by-default repository-maintenance orchestrator. It does not own the Rust engine, release provenance contract, release installer verification, or PE-6 recovery semantics.

It does touch CI workflows/scripts/tests and several active documents. Therefore:

- do not copy or reimplement its orchestrator;
- do not modify `scripts/agent-control/` or `.github/workflows/agent-*.yml` as part of PE-5/PE-6;
- inspect its actual state before each packet;
- if #207 merges, refresh `main` before continuing;
- if #207 remains open, keep PE-5/PE-6 PRs off its owned paths where practical;
- before #207 merges, it must rebase or refresh from current `main` and preserve this PE-5/PE-6 plan and final PE-4 evidence;
- changes to `.github/workflows/tests.yml` or shared CI verification must be reconciled explicitly rather than overwritten.

## Grouped PR Execution Boundaries

The packet definitions below remain required internal milestones and acceptance coverage. They are grouped into the following review boundaries to reduce serial CI and PR overhead without changing their normative order:

- `PE5-IMPLEMENT-1` owns `PE5-CONTRACT-1` through `PE5-PUBLISH-1` on one branch and one PR. The milestones are completed in order with focused local checks; full repository validation and exact-head CI run only on the complete grouped head.
- `PE5-CLOSE-1` remains a separate independent audit/repair/acceptance PR for the merged PE-5 chain.
- `PE6-IMPLEMENT-1` owns `PE6-INVARIANTS-1` through `PE6-EVIDENCE-1` on one branch and one PR, with the same local-milestone and final-head CI rule.
- `PE6-CLOSE-1` remains a separate independent audit/repair/acceptance PR for the merged PE-6 chain.

No internal milestone receives a separate PR, merge, or full CI wait. Final packet-state documentation for a grouped implementation is included in that implementation's reviewed head before exact-head CI.

## Stage Status

| Stage | Priority | Capability | State |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | `COMPLETE` and acceptance-sealed |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | `COMPLETE` and acceptance-sealed |
| PE-3 | P1 | Operator Decision Center | `COMPLETE` and independently acceptance-sealed |
| PE-4 | P1/P2 | Trace-backed Policy Replay | `COMPLETE` and acceptance-sealed under PE4-POST-CLOSE-REPAIR-1 |
| PE-5 | P1.5 | Release Provenance | Complete and independently acceptance-sealed under grouped `PE5-IMPLEMENT-1` and `PE5-CLOSE-1`; internal milestones remain ordered and complete |
| PE-6 | P2 | Fault Injection and Recovery Drills | Grouped `PE6-IMPLEMENT-1` implementation finished; independent `PE6-CLOSE-1` is ready for audit |

# PE-5 — Release Provenance

## Stage Invariants

PE-5 extends the existing release workflow, package builders, installer/upgrader, container build, dependency locks, audit, and atomic rollback paths. It must not create a second release pipeline or artifact truth source.

Release provenance is derived evidence, not proof supplied by a caller. Every accepted release subject must bind, where applicable:

- repository identity and source commit;
- release tag or explicit non-publishing dry-run identity;
- workflow file identity, workflow run, job, and builder environment;
- target OS/architecture and package kind;
- dependency lockfile hashes and build-input hashes;
- release artifact digest, size, media type, and filename;
- SBOM digest and schema;
- provenance/attestation digest and predicate type;
- signing or attestation identity and verification policy;
- verification result, bounded reason codes, and rollback target.

Production signing identity must be external and ephemeral, preferably workload-identity/OIDC-backed when the existing platform supports it. Persistent private signing keys, exported key material, repository secrets containing private keys, and signing credentials copied to self-hosted workers are forbidden.

Tests may use clearly isolated fixture identities and disposable keys that can never authorize a production release.

A release must fail closed when required source, workflow, builder, target, dependency, artifact, SBOM, attestation, signature, or identity binding is missing, malformed, oversized, stale, inconsistent, or untrusted.

No packet in PE-5 authorizes an actual public release, tag creation, deployment, package publication, or installer rollout unless the repository already has that authority and the user explicitly authorizes the external action. Dry-run and local verification are sufficient for implementation acceptance where external publication is not authorized.

## Packet PE5-CONTRACT-1 — Release subject, evidence, and trust contract

**State:** `COMPLETE`

**Prerequisite:** PE-4 final acceptance above; no conflicting release-provenance PR; inspect PR #207 and refresh from current `main`.

**Goal:** Define one versioned, bounded, deterministic release-provenance contract and threat model over the existing release/build/install/upgrade owners before generating or enforcing new evidence.

**Owning paths:**

- `.github/workflows/release.yml` and existing release workflow helpers;
- existing release, packaging, install, upgrade, rollback, checksum, and container-build scripts;
- dependency lockfiles and target packaging metadata;
- `docs/ARCHITECTURE_BOOK.md`, `docs/MODULE_MAP.md`, and focused contract tests.

**Allowed changes:** Contract structs/schemas, canonical serialization and hashing, bounded validators, reason-code taxonomy, fixtures, threat/failure analysis, and additive test helpers.

**Forbidden changes:** No real signing, artifact publication, tag creation, installer enforcement, production secret, persistent private key, release upload, deployment, database migration, API/SDK/Dashboard work, or second release owner.

**Contract requirements:**

- define a canonical release-subject schema and a provenance verification-result schema;
- distinguish `verified`, `invalid`, `insufficient_evidence`, `untrusted_identity`, and `unsupported` outcomes;
- define source, workflow, builder, target, dependency, artifact, SBOM, attestation, signer, and rollback bindings;
- define maximum bytes, collection sizes, identifier lengths, nesting, artifact counts, target counts, and reason counts;
- define deterministic ordering and hashes without serialization fallback or runtime panic;
- define production versus fixture signer identity and prevent fixture evidence from authorizing release;
- define compatibility and version-bump rules;
- define external-action and credential hard stops.

**Verification:** Contract, serde/canonicalization, tamper, missing-field, oversized, target mismatch, fixture-identity, unsupported-schema, deterministic ordering, and no-panic tests; full applicable repository checks.

**Compatibility:** Contract-only and additive. Existing releases and installers remain unchanged and non-provenance-authorizing.

**Rollback:** Revert the contract PR; no artifact or data cleanup.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE5-IMPLEMENT-1`; do not open or merge an internal-milestone PR. Record its final state in the grouped implementation head before the one exact-head CI run.

## Packet PE5-SBOM-1 — Deterministic artifact and container SBOMs

**State:** `COMPLETE`

**Prerequisite:** PE5-CONTRACT-1 complete.

**Goal:** Generate deterministic, bounded, target-correct SBOM evidence for every supported release package and container subject through existing build owners.

**Owning paths:** Existing release/package/container build scripts and workflow; Cargo/Bun/Python dependency locks; release fixtures and tests.

**Allowed changes:** Deterministic SBOM generation, normalization, validation, artifact naming, digest binding, dry-run output, and CI/release workflow integration.

**Forbidden changes:** No signing, publication, installer enforcement, live deployment, dependency mutation solely to alter SBOM output, hidden network dependency in tests, or second packaging pipeline.

**Contract:**

- choose one canonical machine-readable SBOM format and version; additional formats are optional and non-authoritative unless separately versioned;
- bind SBOM to exact source commit, target tuple, package kind, artifact digest, dependency lock hashes, and generator version;
- normalize timestamps and unstable fields or explicitly exclude them from the canonical digest;
- fail closed on missing package subjects, duplicate components, invalid licenses/identifiers where required by the contract, target mismatch, oversized output, or digest mismatch;
- never include secrets, environment tokens, source payloads, or absolute private paths.

**Verification:** Determinism across repeated builds, changed-dependency sensitivity, target/package coverage, container/package parity where applicable, malformed/tampered SBOM, no-secret/path leakage, offline fixture behavior, and workflow dry run.

**Rollback:** Revert SBOM integration; existing package build remains available only if the accepted contract explicitly permits a non-provenance development build. No accepted release may silently omit required SBOM evidence.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE5-IMPLEMENT-1`; retain the ordered evidence in the grouped implementation head.

## Packet PE5-ATTEST-1 — External ephemeral signing and provenance attestations

**State:** `COMPLETE`

**Prerequisite:** PE5-SBOM-1 complete.

**Goal:** Produce and verify signed provenance/attestation evidence for release subjects using an external ephemeral production identity and isolated non-authorizing test fixtures.

**Owning paths:** Existing release workflow, official pinned signing/attestation tooling selected from current supported platform capabilities, release contract implementation, and verification fixtures.

**Allowed changes:** OIDC/workload-identity permissions when required, pinned official actions/tools, provenance predicates, signing/attestation generation, identity policy, certificate/transparency metadata validation, and offline fixture verification.

**Forbidden changes:** No persistent private key, repository-stored signing key, exported identity token, signing on Vader, unsigned fallback presented as verified, unpinned third-party action, public release, or silent trust-on-first-use.

**Contract:**

- bind source commit, workflow identity/ref, run/job, builder, target, artifact and SBOM digests, predicate schema, and signing identity;
- production acceptance requires the configured trusted issuer/subject/workflow policy and ephemeral identity;
- fixture signatures are explicitly marked non-production and cannot pass production policy;
- missing identity, wrong issuer/subject, wrong repository/ref/workflow, expired/not-yet-valid certificate, digest mismatch, malformed bundle, unavailable required transparency evidence, or unsupported predicate fails closed;
- verification remains possible without exposing signing credentials.

**Verification:** Valid fixture, wrong identity, wrong repository/ref/workflow, artifact/SBOM tamper, expired/invalid certificate metadata, malformed attestation, unsupported predicate, missing transparency/identity evidence, and deterministic policy tests. Exercise the production workflow in non-publishing dry-run form when external identity is available.

**Rollback:** Revert attestation integration; revoke or disable external trust policy if necessary. No private-key cleanup should be required because production private keys are forbidden.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE5-IMPLEMENT-1`; retain the ordered evidence in the grouped implementation head.

## Packet PE5-VERIFY-1 — Fail-closed installer and upgrader verification

**State:** `COMPLETE`

**Prerequisite:** PE5-ATTEST-1 complete.

**Goal:** Require complete accepted provenance before an existing installer or upgrader activates a release, while preserving atomic rollback.

**Owning paths:** Existing install/upgrade/checksum/atomic-swap/rollback scripts, release bundle layout, verification CLI/helpers, and focused shell/Python/Rust tests as appropriate.

**Allowed changes:** Pre-install verification, explicit development-mode bypass only when existing policy permits and clearly non-production, bounded evidence reporting, staged extraction, atomic activation, and rollback tests.

**Forbidden changes:** No partial activation before verification, no warning-only production bypass, no network-fetched executable verifier without pinning, no mutation of the previous installation before the new release verifies, no deletion of rollback state, and no implicit trust based only on filename/tag/checksum.

**Contract:**

- verify exact artifact digest, SBOM, provenance/attestation, signer identity, source/workflow/target binding, package inventory, and bundle completeness before activation;
- extract and validate in a staging location;
- preserve the previous known-good installation until post-activation health succeeds;
- rollback atomically on verification, extraction, activation, health-check, permission, or interruption failure;
- record bounded non-secret verification and rollback evidence;
- repeated install/upgrade attempts are deterministic and idempotent where existing semantics permit.

**Verification:** Valid install, tampered artifact, tampered SBOM, wrong target, wrong signer/workflow, missing file, path traversal, permission failure, interrupted extraction, activation failure, failed health check, rollback failure handling, repeat upgrade, and cleanup tests.

**Compatibility:** Existing development builds remain explicitly distinguishable. Existing accepted installations remain usable; upgrading them requires the new verified release contract rather than silently fabricating old provenance.

**Rollback:** Revert enforcement and installer changes only through a reviewed rollback that does not label unverified releases as verified. Preserve prior known-good installations and evidence.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE5-IMPLEMENT-1`; final packet states and closeout routing are committed before the grouped implementation's exact-head CI.

## Packet PE5-PUBLISH-1 — Release workflow ordering and publish gate

**State:** `COMPLETE`

**Prerequisite:** PE5-VERIFY-1 complete.

**Goal:** Integrate build, SBOM, attestation, verification, packaging, and publication ordering into the existing release workflow so no release artifact is published before its complete evidence verifies.

**Owning paths:** `.github/workflows/release.yml`, existing release scripts, artifact upload steps, release dry-run fixtures, action-pin/security checks, and release runbook.

**Allowed changes:** Least-privilege workflow permissions, exact artifact handoff, deterministic manifest assembly, pre-publication verification, dry-run mode, bounded artifact retention, and release evidence summary.

**Forbidden changes:** No automatic tag creation or public release without existing authority and explicit user authorization; no mutable latest-only identity; no unverified artifact upload; no broad token permissions; no persistent credential on self-hosted runners; no weakening of atomic installer rollback.

**Contract:**

- exact source/tag/ref and workflow identity are checked at every handoff;
- build outputs are immutable by digest between SBOM, attestation, verification, and publish steps;
- publication is downstream of successful verification and cannot reuse evidence from another head/run/target;
- cancelled, retried, duplicate, or stale workflow runs cannot publish;
- dry-run produces the complete bundle and verification evidence without external publication;
- permissions are job-minimal and all actions are SHA-pinned.

**Verification:** Workflow syntax/action pins, dry-run end to end, moved-head/tag mismatch, duplicate/stale run, artifact substitution, missing target, failed signing, failed verification, cancelled job, and retry behavior. No real release is required unless separately authorized.

**Rollback:** Disable publication, retain verified bundles/evidence, revert workflow integration, and preserve the previous release path only for non-production development artifacts until repaired.

**Completion:** Merge exact-head green CI, refresh `main`, mark PE5-CLOSE-1 ready.

## Packet PE5-CLOSE-1 — Independent release-provenance acceptance seal

**State:** `COMPLETE`

**Prerequisite:** PE5-PUBLISH-1 complete in merged PR #210.

**Goal:** Independently audit and acceptance-seal the full PE-5 chain without performing an unauthorized public release.

**Audit:** Contract/versioning, deterministic SBOMs, source/workflow/builder/target/dependency/artifact binding, ephemeral identity, fixture non-authorization, attestation verification, installer fail-closed behavior, atomic activation/rollback, workflow ordering, permissions, action pins, secret handling, compatibility, dry-run evidence, cancellation/retry/stale-run behavior, and residual risk.

**Acceptance:** Repair any demonstrated defect before closeout. Exact final-head full CI and post-merge `main` CI must pass. A complete non-publishing release dry run must produce a verified bundle for every supported target/package class. No production private key or real release action may be used merely to satisfy closeout.

**Rollback:** Revert individual PE-5 PRs in reverse dependency order; disable publication first; preserve verified evidence and previous known-good installations.

**Completion:** Complete. PR #211 independently sealed PE-5 at merge `8f830f6772fce8f7cc7a67f38a8773ad3b0d1f56`; exact-head CI run `29228454008` and post-merge `main` CI run `29228989142` passed all seven required jobs. PE6-INVARIANTS-1 is activated without an external release action.

# PE-6 — Fault Injection and Recovery Drills

## Stage Invariants

PE-6 validates existing recovery behavior; it does not create a second scheduler, storage engine, provider authority, release owner, audit system, or rollback mechanism.

All faults must run only against disposable local resources, temporary directories, isolated SQLite databases, ephemeral PostgreSQL containers, fake/stub providers, synthetic repositories/worktrees, and non-publishing release bundles.

Every drill must define:

- subsystem and owner;
- normal-state invariant;
- injected failure and exact injection point;
- expected detection and bounded timeout;
- recovery-success invariant;
- rollback-success invariant;
- data-integrity and audit invariant;
- idempotency/restart/concurrency invariant where applicable;
- abort/kill condition;
- cleanup verification;
- versioned bounded evidence and reason codes.

No destructive external provider call, production database corruption, real target-repository damage, real release publication, credential revocation, or host-level fault outside an isolated approved sandbox is authorized.

## Packet PE6-INVARIANTS-1 — Recovery invariant and drill contract

**State:** `COMPLETE`

**Prerequisite:** PE5-CLOSE-1 complete.

**Goal:** Define the versioned bounded fault-scenario, drill-result, recovery-evidence, and cleanup contracts across existing subsystem owners before injecting failures.

**Owning paths:** Existing engine fault seams and tests; storage/provider/workflow/scheduler/executor/release recovery owners; backup/restore and upgrade rollback scripts; architecture/module docs; focused contract tests.

**Allowed changes:** Scenario/result schemas, capability allowlists, deterministic seed semantics, fault-point registry, timeouts, evidence references/hashes, reason codes, fixtures, and pure validators.

**Forbidden changes:** No fault execution against non-disposable resources, no production mutation, no new recovery authority, no provider call, no host reboot/process kill outside a controlled child process, no schema migration, and no Dashboard/API work.

**Contract:**

- define `fault_scenario`, `fault_drill_result`, and recovery/cleanup evidence versions;
- require explicit disposable-resource identity and capability allowlist;
- distinguish `passed`, `failed_recovery`, `failed_rollback`, `cleanup_failed`, `unsupported`, `invalid_scenario`, and `aborted`;
- bind scenario, seed, subsystem, source head, test environment, injected fault, observations, recovery actions, integrity checks, audit evidence, and cleanup evidence;
- bound duration, retries, processes, files, bytes, events, references, and output size;
- fail closed on unknown fault point, non-disposable target, missing cleanup, evidence tamper, timeout, or invariant ambiguity.

**Verification:** Contract/canonicalization, unsupported fault, non-disposable target, timeout, duplicate scenario, deterministic seed, evidence tamper, cleanup omission, oversized input/result, and no-panic tests.

**Rollback:** Revert the contract PR; no live state or migration.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE6-IMPLEMENT-1`; do not open or merge an internal-milestone PR.

## Packet PE6-HARNESS-1 — Deterministic bounded fault-injection harness

**State:** `COMPLETE`

**Prerequisite:** PE6-INVARIANTS-1 complete.

**Goal:** Implement one reusable allowlisted harness that injects deterministic faults, observes existing recovery owners, always attempts bounded cleanup, and emits versioned evidence.

**Owning paths:** Focused test/support modules under existing Rust/Python/shell test ownership; CI test tooling; no production runtime entrypoint unless an existing test-only seam requires one.

**Allowed changes:** Test-only fault adapters, deterministic clock/random/IO seams, child-process control, disposable resource provisioning, cleanup guards, report generation, and harness tests.

**Forbidden changes:** No generic arbitrary command execution, unrestricted file/network/process fault, production endpoint, persistent daemon, provider credential, main-branch mutation, or second orchestration framework.

**Contract:**

- only registered scenario IDs and injection points execute;
- resources must be created by the harness or explicitly prove disposable ownership;
- fault duration and retries are capped;
- emergency abort and finally-style cleanup always run;
- cleanup failure is a first-class failed outcome;
- evidence is deterministic for equivalent scenario/seed/input;
- concurrent drills cannot share mutable resource identity.

**Verification:** Unknown scenario, traversal/command injection, timeout, process crash, interrupted harness, cleanup failure, concurrent isolation, deterministic replay, and artifact bounds.

**Rollback:** Revert harness/test seams and remove disposable artifacts. No production data cleanup.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE6-IMPLEMENT-1`; retain the ordered evidence in the grouped implementation head.

## Packet PE6-STORAGE-1 — SQLite/PostgreSQL integrity, interruption, backup, and restore drills

**State:** `COMPLETE`

**Prerequisite:** PE6-HARNESS-1 complete.

**Goal:** Prove existing storage transactions, migrations, integrity checks, backup/restore, idempotency, and rollback behavior under bounded failures.

**Owning paths:** `LocalProductStore`, SQLite/PostgreSQL migrations and integration tests, backup/restore scripts, audit/integrity tables, temporary databases and ephemeral PostgreSQL containers.

**Required drill classes:**

- failure before commit, during multi-owner transaction, and after commit acknowledgement;
- duplicate/replayed write and concurrent conflicting write;
- migration interruption and restart;
- integrity/hash tamper detection;
- backup interruption, incomplete backup, restore into clean target, and restore verification;
- connection loss/timeout/deadlock or equivalent PostgreSQL failure using controlled test facilities;
- cleanup and container/database isolation.

**Forbidden:** No corruption of a developer's real database, no deletion of non-harness paths, no down-migration invented solely for the drill, and no acceptance based only on absence of panic.

**Acceptance:** Atomicity, no partial authority state, deterministic retry/idempotency, integrity refusal, restart behavior, backup completeness, restore equivalence, audit preservation, and cleanup all verify for SQLite and PostgreSQL where applicable.

**Rollback:** Revert test seams and harness scenarios; preserve production storage behavior.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE6-IMPLEMENT-1`; retain the ordered evidence in the grouped implementation head.

## Packet PE6-WORKFLOW-1 — Workflow, scheduler, executor, pause, and compensation drills

**State:** `COMPLETE`

**Prerequisite:** PE6-STORAGE-1 complete.

**Goal:** Exercise existing workflow/scheduler/executor recovery and compensation under crashes, stale state, duplication, timeout, concurrency, and restart.

**Owning paths:** Existing workflow runs, scheduler, node executor, executor pool, approvals, operator actions, pause/resume/retry, audit, and compensation tests.

**Required drill classes:**

- crash before/after state transition and audit binding;
- duplicate dispatch/tick/action and stale lease/head/state;
- executor timeout, worker loss, unavailable capacity, and retry exhaustion;
- concurrent approval/reject/resume/retry/pause attempts;
- pause or audit failure requiring compensation;
- restart with in-flight, blocked, failed, cancelled, or completed runs;
- rollback or recovery owner unavailable;
- cleanup of temporary worktrees/processes/resources.

**Acceptance:** No duplicate authority, no impossible terminal transition, audit/state consistency, exact idempotency, bounded retries, fail-closed unavailable owners, tested compensation, restart recovery, and cleanup evidence.

**Rollback:** Revert test seams/scenarios only; existing runtime owners remain unchanged unless a demonstrated defect requires a separate bounded repair in the same packet PR.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE6-IMPLEMENT-1`; retain the ordered evidence in the grouped implementation head.

## Packet PE6-PROVIDER-1 — Provider, budget, audit, timeout, and kill-control drills

**State:** `COMPLETE`

**Prerequisite:** PE6-WORKFLOW-1 complete.

**Goal:** Validate existing provider/budget/audit safety behavior with fake or stub providers under bounded failures and contradictory usage evidence.

**Owning paths:** Existing provider adapters, `FakeProvider`, pricing/budget reservation, persistent redacted audit, local runner provider, timeout and kill controls, and focused tests.

**Required drill classes:**

- timeout, cancellation, malformed response, partial stream/result, and provider exception;
- missing/invalid pricing, reservation failure, actual usage over/under reservation, and incomplete usage;
- audit write failure before and after provider result;
- redaction failure or sensitive-pattern detection;
- kill switch before call, during bounded call, and before result acceptance;
- retry with ambiguous provider outcome;
- restart/reconciliation of reserved and posted cost evidence.

**Forbidden:** No real provider call, no live credential, no unbounded sleep/network dependency, no silent model substitution, and no fabricated usage/pricing evidence.

**Acceptance:** No unauthorized call, bounded timeout/cancellation, fail-closed pricing/audit/redaction, deterministic reservation reconciliation, no secret leakage, kill control effectiveness, explicit ambiguous outcome, and cleanup.

**Rollback:** Revert test seams/scenarios; existing provider and budget authority remain unchanged.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE6-IMPLEMENT-1`; retain the ordered evidence in the grouped implementation head.

## Packet PE6-RELEASE-1 — Provenance, installer, upgrade, and rollback drills

**State:** `COMPLETE`

**Prerequisite:** PE6-PROVIDER-1 complete and PE-5 acceptance remains valid.

**Goal:** Prove the accepted PE-5 release chain refuses invalid evidence and recovers atomically from bounded package, verification, activation, and health failures.

**Owning paths:** PE-5 release contract, SBOM/attestation verifier, installer/upgrader, atomic swap/rollback, release dry-run workflow fixtures, and temporary installation roots.

**Required drill classes:**

- missing/tampered artifact, SBOM, attestation, or manifest;
- wrong source/workflow/signer/target binding;
- interrupted download/extraction/staging;
- permission and disk-space simulation within a disposable root;
- activation interruption and failed post-activation health check;
- rollback interruption, repeated rollback, and previous-version preservation;
- stale/duplicate/cancelled release workflow evidence;
- cleanup of staged and failed artifacts.

**Forbidden:** No public release, real tag, deployment, system installation outside a disposable root, real signing credential, or deletion of the host's current installation.

**Acceptance:** Verification always precedes activation; invalid evidence cannot install; previous known-good state survives every failed path; rollback is deterministic/idempotent; incomplete cleanup is visible and fails the drill.

**Rollback:** Revert drill scenarios/test seams; do not weaken PE-5 enforcement to make drills pass.

**Completion within grouped implementation:** Complete and locally verify this milestone on `PE6-IMPLEMENT-1`; final packet states and closeout routing are committed before the grouped implementation's exact-head CI.

## Packet PE6-EVIDENCE-1 — Drill registry, reports, CI execution, and operator inspection

**State:** `COMPLETE`

**Prerequisite:** PE6-RELEASE-1 complete.

**Goal:** Provide one deterministic registry and bounded report path for running supported drill sets locally and in CI and inspecting results without adding runtime mutation authority.

**Owning paths:** Existing test/CI tooling, harness registry/report implementation, CI artifacts, and `docs/RUNBOOK.md`. Add API/SDK/Dashboard surfaces only if repository evidence demonstrates a concrete operator need and the packet is updated before implementation.

**Allowed changes:** Scenario registry, filtered local CLI, CI matrix/shards, bounded JSON report, human-readable summary, one-day or similarly bounded CI artifacts, and runbook procedures.

**Forbidden changes:** No always-on daemon, scheduler, production API mutation, database truth source, automatic destructive drill, release publication, or arbitrary scenario input from untrusted callers.

**Contract:**

- registry entries bind exact scenario versions, owners, required capabilities, timeout, and supported environments;
- CLI/CI accept allowlisted IDs or named suites only;
- reports include all invariants, observations, recovery/rollback/cleanup results, evidence hashes, durations, and reason codes;
- CI failures are attributable to exact scenarios and never silently retried into success;
- unsupported environment/capability is explicit, not a pass;
- reports are bounded, redacted, deterministic, and non-authoritative outside their exact source head/environment.

**Verification:** Registry drift, unknown suite, filtering, sharding, deterministic report, failed/aborted/unsupported states, artifact bounds/redaction, CI wiring, and runbook command tests.

**Rollback:** Revert registry/report/CI integration; subsystem recovery implementations remain intact.

**Completion:** Merge exact-head green CI, refresh `main`, mark PE6-CLOSE-1 ready.

## Packet PE6-CLOSE-1 — Independent recovery-drill acceptance seal

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE6-EVIDENCE-1 complete.

**Goal:** Independently audit and acceptance-seal PE-6 and the completed PE-5/PE-6 recovery chain.

**Audit:** Invariant completeness, harness capability boundary, deterministic faults, disposable-resource enforcement, timeout/abort, cleanup, SQLite/PostgreSQL integrity, migration/backup/restore, workflow/scheduler/executor concurrency and compensation, provider/budget/audit/kill behavior, PE-5 provenance/install/rollback drills, registry/report/CI correctness, no external destructive action, compatibility, and residual risk.

**Acceptance:**

- every registered supported drill has a deterministic pass and focused negative-path evidence;
- cleanup is proven, not assumed;
- no test can target a non-disposable resource without failing closed;
- all existing runtime/release authority remains with its original owner;
- exact final-head full CI and post-merge `main` CI pass;
- any demonstrated defect is repaired and re-tested before closeout;
- no public release, deployment, real provider call, or host-level destructive action was used for acceptance.

**Rollback:** Revert individual PE-6 PRs in reverse dependency order; disable drill CI entrypoints first; retain evidence and all existing production recovery behavior.

**Completion:** Mark all PE-6 packets complete, record exact PR/commit/CI evidence, synchronize active documents, and report remaining product gaps without inventing PE-7.

## Active Routing

1. Open the single grouped PE-6 implementation PR only after final local validation, then merge its exact green head and verify post-merge `main` CI.
2. Independently audit and merge `PE6-CLOSE-1`, then verify post-merge `main` CI.
3. Refresh `main`, active documents, open PRs, and CI after every grouped merge.
4. Keep #207's orchestrator lane separate; reconcile shared CI/docs instead of overwriting either lane.
5. Do not create PE-7, a second release owner, a second recovery owner, or a new runtime/control plane during this objective.
