# Module Contract — module_001: Schema Template Module

Source: harness_architecture_book_v0.7.4.1-canonical §6.5

Covers: item_001 (template fill) + item_002 (schema bugfix)

```yaml
module_contract:
  module_id: module_001
  title: "Schema Template Module"
  objective: "补全 task_spec.json 模板字段，并修复 project_board.md 中的 schema inconsistency"
  input_interface:
    description: "5 个 task 目录下的 task_spec.json 模板文件（含 _template: true）+ project_board.md 当前版本"
    format: "JSON (task_spec.json) + YAML-in-Markdown (project_board.md)"
    source: "docs/stage0/tasks/*/task_spec.json, docs/stage0/project_board.md"
  output_interface:
    description: "5 个 task_spec.json 字段已填写 + project_board.md inconsistency 已修复 + run_log 记录"
    format: "JSON + YAML-in-Markdown + Markdown"
    destination: "docs/stage0/tasks/*/task_spec.json, docs/stage0/project_board.md, docs/stage0/tasks/task-002-bugfix/run_log.md"
  acceptance_criteria:
    - "5 个 task_spec.json 的 project_id、source_project_item、objective、risk_level 字段已填写"
    - "_template: true 标记保留，不伪造完成状态"
    - "project_board.md 中的 schema inconsistency 被识别并修复"
    - "task-002-bugfix/run_log.md 记录 bug 复现路径和修复原因"
    - "所有 Project Board item 状态回写正确"
  required_tests:
    - pack_module_001
  context_budget:
    preferred_tokens: 80000
    max_tokens: 200000
  dependencies: []
  allowed_files:
    - docs/stage0/tasks/task-001-code-small/task_spec.json
    - docs/stage0/tasks/task-002-bugfix/task_spec.json
    - docs/stage0/tasks/task-003-doc-update/task_spec.json
    - docs/stage0/tasks/task-004-config-rule/task_spec.json
    - docs/stage0/tasks/task-005-failure-fix-loop/task_spec.json
    - docs/stage0/project_board.md
    - docs/stage0/tasks/task-002-bugfix/run_log.md
    - docs/stage0/tasks/task-002-bugfix/completion.json
    - docs/stage0/tasks/task-002-bugfix/events.jsonl
    - docs/stage0/tasks/task-002-bugfix/handoff_pack.json
  forbidden_files:
    - src/
    - tests/
    - runtime/
    - .runtime/
    - .git/
  risk_level: low
  integration_points:
    - "project_board.md 状态回写"
    - "task_spec.json → task-001 运行输入"
  rollback_policy: "如发现填写错误，回退 task_spec.json 到 _template 版本（git checkout 或手动还原）"
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
