# Contributing to Token-Efficient Agent Harness Lab

Thanks for your interest. This guide covers setup, focused verification, and pull requests.

## Prerequisites

- **Rust** stable toolchain ([rustup](https://rustup.rs/) recommended)
- **Bun** for dashboard and TypeScript SDK verification
- **Python 3.11+** for the Python SDK; repository scripts generally run via [uv](https://docs.astral.sh/uv/)
- **Git**

## Getting started

```bash
git clone https://github.com/Igzela/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab

cargo build -p engine

cd dashboard
bun install --frozen-lockfile
bun run build:static
cd ..
```

Optional readiness:

```bash
uv run --no-project python scripts/acp_local_doctor.py
```

## Verification model

Do **not** hand-maintain “N tests pass” claims in docs. CI and release evidence report current counts.

Contributors run **focused** checks for the surfaces they change. Full matrix verification is CI’s job (seven required jobs on the exact PR head).

### Docs only

```bash
git diff --check
uv run --no-project python scripts/check_agent_handoff.py
```

### Python SDK (`sdk/python/`)

```bash
cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
```

(Uses `unittest`, not pytest.)

### TypeScript SDK (`sdk/typescript/`)

```bash
cd sdk/typescript && bun install --frozen-lockfile && bun run test && bun run build
```

### Dashboard (`dashboard/`)

```bash
cd dashboard && bun install --frozen-lockfile && bun run lint && bun run typecheck && bun run build
```

### Rust engine (`engine/`)

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
cargo test -p engine
```

PostgreSQL integration (when you touch storage parity):

```bash
cargo test -p engine --features pg-tests -- --test-threads=1
```

### Workflows / orchestrator scripts

```bash
uv run --no-project --with pyyaml python scripts/check_agent_workflow_yaml.py
PYTHONPATH=scripts/agent-control uv run --no-project python -m unittest \
  tests/test_agent_control_ci.py \
  tests/test_agent_control_dry_run.py \
  tests/test_agent_control_state.py \
  tests/test_agent_control_worktree.py \
  tests/test_agent_orchestrator_repairs.py \
  tests/test_agent_orchestrator_artifacts.py \
  tests/test_agent_review_finalization.py
uv run --no-project python tools/check_security_baseline.py
```

### Full local baseline (maintainers / large changes)

See `AGENTS.md` Verification Baseline. Prefer CI for the complete matrix unless you are changing cross-stack contracts.

Always before commit:

```bash
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

## Code style

- **Rust:** `cargo fmt`, `cargo clippy` with zero warnings. Comments only when the reason is non-obvious.
- **TypeScript:** strict mode; avoid `any`.
- **Python:** dataclasses for schemas in the deterministic kernel; no pydantic there. Prefer `unittest` as in the tree today.
- **Commit messages:** English, concise, focus on *why*.

## Pull request process

1. Branch from latest `main` (or continue an owned PR).
2. Keep the PR one coherent change.
3. Run focused checks for the surfaces you touched.
4. Open a PR targeting `main`. Required CI must be green on the **exact** reviewed head (unless a strict documentation-only exception applies; see `docs/REAL_WORLD_TESTING_PLAYBOOK.md`).
5. Describe goal, scope, tests, compatibility, and rollback.
6. Auto-merge stays off by default; maintainers merge when classifier, CI, and review allow.

Default daily path for maintainers and agents: local Agent → focused branch → PR → exact-head CI → independent review → manual squash merge.

## Safety boundaries

Keep changes testable, observable, and rollbackable. By default do not:

- add real provider POSTs without identity, pricing, budget, and receipt contracts;
- weaken fail-closed auth, audit, budget, or exact-head evidence;
- write registered target `main`, merge, deploy, or release without explicit authority;
- invent CI or implementation evidence.

Target-repository output exists only behind explicit gates (`ACP_ENABLE_TARGET_REPO_OUTPUT`, approvals, allowlists, kill switch). It may produce controlled branch/patch/PR paths; it does not authorize silent target-`main` writes.

## Reporting issues

Use [issue forms](https://github.com/Igzela/token-efficient-agent-harness-lab/issues/new/choose):

- Bug report
- Feature request
- External validation (clean install / docs dry-run without code)

Look for the `good first issue` label for starter work. Each starter issue should state allowed paths, non-goals, focused checks, and acceptance criteria.

Security vulnerabilities: see [SECURITY.md](SECURITY.md) (private advisory path — not public issues).

Conduct / harassment: see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) (private contact — not the public issue tracker).

General help and response policy: [SUPPORT.md](SUPPORT.md).

Forward plan: [docs/NEXT_DECISION.md](docs/NEXT_DECISION.md) only. Do not add a second roadmap document.

## Dependency updates

Dependabot is **not** enabled by default. Automatic dependency PRs can thrash CI and conflict with pinned Actions / release attestation work. Propose dependency bumps as manual, reviewed PRs with focused evidence until a maintainer documents an explicit Dependabot policy.

## Citation and changelog

- Cite with [CITATION.cff](CITATION.cff)
- User-facing notes: [CHANGELOG.md](CHANGELOG.md)

## License

MIT. By contributing, you agree your contributions are licensed under the same terms.
