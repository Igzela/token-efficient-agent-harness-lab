## Summary

<!-- What changed and why. Link related Issue if any. -->

- **task_slice_id / task_packet_id (if any):**
- **Change class:** <!-- docs | dashboard | Python SDK | TypeScript SDK | Rust engine | workflow/release/security | mixed -->

## Scope

- **Allowed surfaces touched:**
- **Explicit non-goals:**

## Focused verification (pick the surfaces you changed)

Do **not** claim you ran the full matrix unless you did. CI owns the seven required jobs on the exact head.

- [ ] **docs only** — `git diff --check` · `uv run --no-project python scripts/check_agent_handoff.py`
- [ ] **Python SDK** — `cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests`
- [ ] **TypeScript SDK** — `cd sdk/typescript && bun install --frozen-lockfile && bun run test && bun run build`
- [ ] **Dashboard** — `cd dashboard && bun install --frozen-lockfile && bun run lint && bun run typecheck && bun run build`
- [ ] **Rust engine** — `cargo fmt --all -- --check` · `cargo clippy -p engine --all-targets --all-features -- -D warnings` · `cargo test -p engine`
- [ ] **workflow / orchestrator / security baseline** — see `CONTRIBUTING.md` workflow section · `uv run --no-project python tools/check_security_baseline.py`
- [ ] **handoff** — `uv run --no-project python scripts/check_agent_handoff.py` (always when touching active docs)

## Compatibility and rollback

- **Compatibility:** <!-- preserved / intentionally versioned -->
- **Rollback:** <!-- e.g. revert this PR -->

## Checklist

- [ ] No secrets, raw transcripts, or private paths committed
- [ ] No invented CI or implementation evidence
- [ ] Auto-merge left disabled unless separately authorized
- [ ] Docs-only PRs follow the playbook exception; all other PRs need exact-head required CI green

> Full matrix: GitHub Actions `tests` workflow on the exact PR head.
