You are an autonomous coding agent working on {{REPO_NAME}}.

## Task

Issue #{{ISSUE_NUMBER}}: {{ISSUE_TITLE}}

### Goal and Scope

{{ISSUE_BODY}}

### Repository Context

Current `AGENTS.md`:
```
{{AGENTS_MD}}
```

Current status:
```
{{CURRENT_STATUS}}
```

Next decisions:
```
{{NEXT_DECISION}}
```

Module map:
```
{{MODULE_MAP}}
```

### Instructions

1. Inspect the repository state from the current branch ({{GIT_BRANCH}}).
2. Read the task specification and all referenced prerequisite materials.
3. Implement the smallest coherent solution that satisfies the goal and acceptance criteria.
4. Follow the code conventions, module ownership, and architecture boundaries documented in the repository.
5. Run focused verification:
   - `cargo fmt --all -- --check`
   - `cargo clippy -p engine --all-targets --all-features -- -D warnings`
   - `cargo test -p engine`
   - `PYTHONPATH=src uv run --no-project python -m unittest discover -s tests`
   - `bash scripts/verify_rust_typescript_stack.sh`
   - `bash scripts/check_wire_codegen_drift.sh`
   - `uv run --no-project python tools/check_security_baseline.py`
   - `uv run --no-project python scripts/check_agent_handoff.py`
   - `git diff --check`
6. If the changes touch Docker, migration, release, or concurrency surfaces, add the applicable checks.
7. Commit all changes with clear messages.
8. Push the branch.
9. Create or update a PR.
10. Record the PR number and head SHA as output evidence.

### Constraints

- Do not modify files outside the scope defined in the task.
- Do not weaken existing tests or CI gates.
- Do not commit secrets, credentials, or API keys.
- Do not create new documentation files unless explicitly instructed.
- Do not force-push protected branches.
- Do not merge, tag, release, publish, or deploy.
- Keep the diff minimal and reviewable.
