# Contributing to Token-Efficient Agent Harness Lab

Thanks for your interest in contributing. This document covers how to set up your environment, run tests, and submit changes.

## Prerequisites

- **Rust** stable toolchain (rustup recommended)
- **Bun** (for dashboard and TypeScript builds)
- **Python 3.10+** with **uv** (for SDK and scripts)
- **Git**

## Getting Started

```bash
git clone https://github.com/<org>/token-efficient-agent-harness-lab.git
cd token-efficient-agent-harness-lab

# Build the Rust engine
cargo build -p engine

# Install dashboard dependencies and build
cd dashboard
bun install
bun run build
cd ..
```

## Running Tests

Run the full test suite before submitting any change.

**Rust engine** (primary, 1379+ tests):
```bash
cargo test -p engine
```

**Dashboard TypeScript** (strict mode, static export):
```bash
cd dashboard
bun run build
cd ..
```

**Python SDK**:
```bash
cd sdk/python
uv run python -m pytest
cd ../..
```

**Full verification** (recommended before commit):
```bash
cargo test -p engine
cargo fmt --check
cargo clippy -p engine --all-targets -- -D warnings
cd dashboard && bun run build && cd ..
uv run --no-project python scripts/check_agent_handoff.py
```

## Code Style

- **Rust**: `cargo fmt` for formatting, `cargo clippy` with zero warnings. No comments unless the WHY is non-obvious.
- **TypeScript**: strict mode, readonly where possible, no `any` types.
- **Python**: dataclass schemas only, no pydantic. Python 3.10+ features welcome.
- **Commit messages**: English, concise, focus on *why* the change is made.

## Pull Request Process

1. Fork the repo and create a branch from `main`.
2. Make your changes. Keep PRs focused -- one logical change per PR.
3. Run the full verification suite (see above).
4. Open a PR targeting `main`. CI must pass (7 GitHub Actions jobs) before merge.
5. Describe what changed and why in the PR body.

## Safety Boundaries

This project studies deterministic agent infrastructure. The following are **not allowed** by default:

- Real model-provider API calls in the dispatch kernel
- Cloud SaaS deployment paths
- Autonomous worker processes that act without supervision
- Container/VM/sandbox isolation layers
- Writing to target repositories from the harness

These constraints keep the harness deterministic, auditable, and safe for local experimentation. If you believe a boundary should be discussed, open an issue first.

## Reporting Issues

Use the [GitHub issue templates](../../issues/new/choose) to report bugs or request features. Include reproduction steps, expected behavior, and your environment (OS, Rust version, Bun version).

## License

This project is licensed under the **MIT License**. By contributing, you agree that your contributions will be licensed under the same terms.
