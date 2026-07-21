# Changelog

All notable **user-facing** changes are recorded here.

This project does not hand-maintain test counts. CI and release evidence report current verification results. Detailed packet state lives in [`docs/NEXT_DECISION.md`](docs/NEXT_DECISION.md); operational facts live in [`docs/CURRENT_STATUS.md`](docs/CURRENT_STATUS.md).

Format is inspired by [Keep a Changelog](https://keepachangelog.com/). Versions follow the repository release tags when published.

## [Unreleased]

### Added

- PE7 OpenCode external coding adapter (fixture-first, default-off): `opencode_external` executor + `adapters/opencode/` deny-by-default fixture runner; no live OpenCode binary admission yet.
- PE7 OpenCode fixture-adapter honesty repair: canonical `completed` status, reserved exact-capability routing, full result identity, tool-evidence counters, independent patch grammar, required base/worktree identity, process-group termination, and `FIXTURE_ADAPTER_MANIFEST.json` (real binary admission remains deferred).
- Growth closeout guards: README public-surface checker, site OG metadata + `site/og.svg`, `gh`/attestation doctor checks, in-repo `exact-head-check` workflow.
- Exact-Head CI proof Action: `actions/exact-head-check/` (re-read live PR head, fail closed on move, JSON proof + Job Summary; no merge, no model).
- Five-minute no-provider demo: `./scripts/demo.sh` (fixture dispatch, source-revision proof, stale-head rejection, cleanup).
- Public contributor surface: issue forms (bug, feature, external validation), layered PR template, `SUPPORT.md`, private conduct path, and citation metadata.
- Ship PR default path documentation and fail-closed exact-head CI verification (see PR #240).

### Changed

- README and landing page public entry no longer hard-code temporary orchestrator/Issue state or test counts.
- CONTRIBUTING uses focused verification tiers; documentation contributors are not required to run the full Rust matrix.

### Security

- Security response text uses small-maintainer **goals**, not hard SLAs. Prefer GitHub Security Advisories for vulnerabilities.

## Older history

Earlier capability work (dispatch kernel, PE-1–PE-6, Agent Runtime, orchestrator, recursive execution controls, and related repairs) is recorded in merged PR history and release tags. Prefer GitHub Releases and PR titles for pre-Unreleased detail until the next user-facing tagged release.
