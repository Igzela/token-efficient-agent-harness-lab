# GPT Web to Vader Codex repository-agent smoke test

## Result

The repository-agent smoke path was exercised through the bounded Agent Task in
source Issue #217:

```text
GPT Web → Agent Task #217 → Vader Codex patch → validated patch → GitHub-hosted PR
```

Vader Codex produced the patch for this document as an artifact-only worker. The
GitHub-hosted finalizer validated the patch against the declared scope and
opened the resulting pull request. The pull request remains unmerged.

## Control and provenance

- Source Issue: #217.
- Allowed change: `docs/agent-smoke-test.md` only.
- Patch validation: the bounded artifact and allowed-path checks passed, and
  `git diff --check` passed.
- Finalization: branch push and pull-request creation remained owned by the
  GitHub-hosted finalizer.
- Auto-merge: disabled throughout; no auto-merge or merge action was performed.

This record contains no secrets, tokens, environment values, or raw model logs.
