# Module Contract — module_002: Doc/Config/Failure Module

Source: harness_architecture_book_v0.7.4.1-canonical §6.5

Covers: item_003 (doc update) + item_004 (approval validation) + item_005 (failure loop)

```yaml
module_contract:
  module_id: module_002
  title: "Doc/Config/Failure Module"
  objective: "更新 README 文档、验证 approval_request 模板、用故意失败验证 Advisor Protocol"
  input_interface:
    description: "README.md 当前版本 + task-004 approval_request 模板 + task-005 failure 场景设计"
    format: "Markdown + YAML-in-Markdown"
    source: "docs/stage0/README.md, docs/stage0/tasks/task-004-config-rule/run_log.md, docs/stage0/tasks/task-005-failure-fix-loop/"
  output_interface:
    description: "README.md 更新 + approval_request 验证记录 + Advisor Protocol 调用记录 + Fix Loop 结果"
    format: "Markdown + JSON (events.jsonl, completion.json)"
    destination: "docs/stage0/README.md, docs/stage0/tasks/task-004-*/run_log.md, docs/stage0/tasks/task-005-*/run_log.md"
  acceptance_criteria:
    - "README.md Five Task Templates 表格包含实际项目 item_id 和 objective"
    - "task-004 run_log.md 中 approval_request 模板字段已填写（模拟数据）"
    - "task-004 记录审批流程是否可读"
    - "task-005 至少产生一次 failed_retryable"
    - "task-005 Advisor Protocol 至少调用一次，diagnosis/recommended_action/do_not_do 完整记录"
    - "task-005 Fix Loop 结果记录在 run_log.md"
    - "所有 Project Board item 状态回写正确"
  required_tests:
    - pack_module_002
  context_budget:
    preferred_tokens: 80000
    max_tokens: 200000
  dependencies: []
  allowed_files:
    - docs/stage0/README.md
    - docs/stage0/tasks/task-003-doc-update/run_log.md
    - docs/stage0/tasks/task-003-doc-update/completion.json
    - docs/stage0/tasks/task-003-doc-update/events.jsonl
    - docs/stage0/tasks/task-003-doc-update/handoff_pack.json
    - docs/stage0/tasks/task-004-config-rule/run_log.md
    - docs/stage0/tasks/task-004-config-rule/task_spec.json
    - docs/stage0/tasks/task-004-config-rule/completion.json
    - docs/stage0/tasks/task-004-config-rule/events.jsonl
    - docs/stage0/tasks/task-004-config-rule/handoff_pack.json
    - docs/stage0/tasks/task-005-failure-fix-loop/run_log.md
    - docs/stage0/tasks/task-005-failure-fix-loop/completion.json
    - docs/stage0/tasks/task-005-failure-fix-loop/events.jsonl
    - docs/stage0/tasks/task-005-failure-fix-loop/handoff_pack.json
  forbidden_files:
    - src/
    - tests/
    - runtime/
    - .runtime/
    - .git/
  risk_level: low
  integration_points:
    - "project_board.md 状态回写"
    - "batch_digest.md 从 task 运行结果派生"
  rollback_policy: "文档类回退用 git checkout；失败循环无回退需求（失败是预期行为）"
```

## Context Budget Contract (§6.5)

```yaml
module_context_contract:
  max_active_context_tokens: 200000
  preferred_context_tokens: 80000
  input_contract_required: true
  output_contract_required: true
  test_case_required: true
  artifact_refs_only: true
```
