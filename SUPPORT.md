# Support

## How to get help

| Need | Where |
|---|---|
| Bug report | [Bug form](https://github.com/Igzela/token-efficient-agent-harness-lab/issues/new?template=bug.yml) |
| Feature idea | [Feature form](https://github.com/Igzela/token-efficient-agent-harness-lab/issues/new?template=feature.yml) |
| Clean-environment dry-run feedback | Run `./scripts/external_validation.sh` (optional `--report path.json`), then [External validation form](https://github.com/Igzela/token-efficient-agent-harness-lab/issues/new?template=external_validation.yml) |
| Security vulnerability | [SECURITY.md](SECURITY.md) — GitHub Security Advisory (preferred) |
| Code of conduct / harassment | Private contact in [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — **not** a public issue |
| Setup / contributing | [CONTRIBUTING.md](CONTRIBUTING.md) and [README.md](README.md) |
| Forward plan | [docs/NEXT_DECISION.md](docs/NEXT_DECISION.md) (single plan — no second roadmap file) |
| Maintainer operations | [AGENTS.md](AGENTS.md), [docs/CURRENT_STATUS.md](docs/CURRENT_STATUS.md), [docs/RUNBOOK.md](docs/RUNBOOK.md) |

## What this tracker is for

Public issues should be **external-facing** bugs, docs problems, feature requests, external validation notes, and `good first issue` work.

Internal repository-agent smoke runs, emergency-stop control, and orchestrator capacity Issues are maintainer operations (`agent-*` labels). They are not the primary place for new user questions.

## Discussions (when enabled)

If GitHub Discussions is enabled for the repository, use:

| Category intent | Use for |
|---|---|
| Q&A | Setup questions that are not bugs |
| Ideas | Early design chat before a feature issue |
| Show and tell | External adoption notes |
| Announcements | Maintainer-only |

Until Discussions is enabled, prefer issue forms above.

## Contributor response policy

This is a small-maintainer research lab. There is **no SLA**.

| Kind | Target |
|---|---|
| Security advisory | Goals in [SECURITY.md](SECURITY.md) |
| Conduct complaint | Private, fair review per [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) |
| Bug / validation issue | Best-effort triage when capacity allows |
| Feature request | May be closed with a pointer to scope boundaries or `docs/NEXT_DECISION.md` |
| `good first issue` PR | Review when focused checks and CI allow; docs-only PRs use the playbook exception |

Silent periods do not mean rejection. Do not escalate by opening duplicate issues.

## Scope of support

In scope: the Rust engine, local dashboard, SDKs, documented scripts, and verified install paths in the README.

Out of scope by default: free remote debugging of private deployments, paid provider account setup, multi-tenant hosting, and unauthenticated production internet exposure.
