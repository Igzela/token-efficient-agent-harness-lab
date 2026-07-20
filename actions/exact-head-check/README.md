# Exact-Head CI Check

Composite GitHub Action that **re-reads the live pull request** and fails closed if the head commit is not exactly the SHA you expected.

- Does **not** merge
- Does **not** call models
- Does **not** require the full Agent Control Plane
- Writes a Job Summary and `exact-head-proof.json` (configurable)

## Use

```yaml
- uses: Igzela/token-efficient-agent-harness-lab/actions/exact-head-check@main
  with:
    github-token: ${{ github.token }}
    pull-request: ${{ github.event.pull_request.number }}
    expected-head: ${{ github.event.pull_request.head.sha }}
```

Pin to a tag or commit SHA in production consumers. `@main` is only for lab experimentation.

## Inputs

| Input | Required | Description |
|---|---|---|
| `github-token` | yes | Token with PR read |
| `pull-request` | yes | PR number |
| `expected-head` | yes | 40-hex commit that must still be the PR head |
| `repository` | no | `owner/name` (default: current repository) |
| `allow-fork-head` | no | `true` to allow fork head repositories (default `false`) |
| `proof-path` | no | Proof JSON path (default `exact-head-proof.json`) |

## Outputs

| Output | Description |
|---|---|
| `live-head` | Observed PR head SHA |
| `status` | `pass` or `fail` |
| `proof-path` | Path to the proof file |

## Fork policy

By default, a fork head repository is rejected. Set `allow-fork-head: true` only when you intentionally verify external contributions.

## Local pure checks

```bash
bash actions/exact-head-check/test_verify_local.sh
```
