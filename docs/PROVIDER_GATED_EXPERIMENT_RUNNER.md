# Provider-Gated Experiment Runner

Status: approved direction for local research.

Real stateful-vs-stateless experiment-runner work is allowed after deterministic pilots. It must remain repo-scoped, local, explicitly opted in, budgeted, observable, killable, testable, and rollbackable.

This work should extend the existing workflow, executor, provider, storage, scorecard, and dashboard modules. It must not create a second Agent Runtime, scheduler, DAG kernel, mailbox, or storage layer.

The first implementation should compare `stateless_reread` with `stateful_store` under the same task, iteration budget, pass criterion, and quality method. It should emit token-efficiency scorecards and read-only comparison evidence.

CI must use stub or deterministic behavior. Real external model execution is for local operator runs only, behind existing trusted-local or legacy execution gates.

A PR for this work must document gates, budget limits, pause/kill behavior, stored evidence shape, authority changes, validation, residual risk, and rollback.
