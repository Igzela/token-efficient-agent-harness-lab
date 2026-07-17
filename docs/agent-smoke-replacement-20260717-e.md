# Replacement repository-agent smoke — 2026-07-17 E

Source Issue: #235.

This non-empty Markdown artifact is the bounded replacement repository-agent
smoke input. Vader must produce it, and artifact validation must accept it,
before the GitHub-hosted finalizer may create the smoke pull request. That pull
request must complete exact-head canonical seven-job CI on the worker-dispatched
head.

After that CI is green, the worker finalizer must dispatch `agent-ci-monitor`
through `workflow_dispatch` with the exact Issue, pull-request, head, and CI-run
bindings. Independent review must use that same exact head and reach a bounded
terminal state.

Auto-merge remained disabled. This smoke exercises no merge authority.
