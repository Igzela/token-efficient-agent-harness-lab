# Project Board

Source: harness_architecture_book_v0.7.4.1-canonical §6.3

Project Board is the project state source, NOT the execution queue.
Task Queue only executes `ready` items.

```yaml
project_board:
  project_id: proj_2026_stage0_schema_validation
  board_version: 1
  status: active
  updated_at: "2026-05-15T22:05:00+08:00"
  items:
    - item_id: item_001
      title: "Fill template fields in task_spec.json files"
      type: module
      status: done
      priority: P1
      owner_agent: ""
      dependencies: []
      input_contract: "5 个 task 目录下的 task_spec.json 模板文件（含 _template: true）"
      output_contract: "5 个 task_spec.json 的 project_id、source_project_item、objective、risk_level 字段已填写，_template 标记保留"
      acceptance_tests:
        - pack_module_001
      context_budget: "preferred 80k, max 200k"
      allowed_files:
        - docs/stage0/tasks/task-001-code-small/task_spec.json
        - docs/stage0/tasks/task-002-bugfix/task_spec.json
        - docs/stage0/tasks/task-003-doc-update/task_spec.json
        - docs/stage0/tasks/task-004-config-rule/task_spec.json
        - docs/stage0/tasks/task-005-failure-fix-loop/task_spec.json
      forbidden_files:
        - src/
        - tests/
        - runtime/
        - .runtime/
        - .git/
      artifact_refs: []
      retry_count: 0
      failure_code: ""
      escalation_policy: "retry once, then block and ask human"

    - item_id: item_002
      title: "Fix schema inconsistency in task_spec.json (missing project_id)"
      type: bug
      status: done
      priority: P1
      owner_agent: ""
      dependencies:
        - item_001
      input_contract: "item_001 review 阶段发现的 task_spec.json 缺少 project_id 字段"
      output_contract: "5 个 task_spec.json 增加 project_id 字段，run_log 记录 bug 复现路径和修复原因"
      acceptance_tests:
        - pack_module_001
      context_budget: "preferred 80k, max 200k"
      allowed_files:
        - docs/stage0/project_board.md
        - docs/stage0/tasks/task-002-bugfix/run_log.md
        - docs/stage0/tasks/task-001-code-small/task_spec.json
        - docs/stage0/tasks/task-002-bugfix/task_spec.json
        - docs/stage0/tasks/task-003-doc-update/task_spec.json
        - docs/stage0/tasks/task-004-config-rule/task_spec.json
        - docs/stage0/tasks/task-005-failure-fix-loop/task_spec.json
      forbidden_files:
        - src/
        - tests/
        - runtime/
        - .runtime/
        - .git/
      artifact_refs: []
      retry_count: 0
      failure_code: ""
      escalation_policy: "invoke Advisor Protocol, then retry"

    - item_id: item_003
      title: "Update README.md Five Task Templates section"
      type: doc
      status: done
      priority: P2
      owner_agent: ""
      dependencies:
        - item_001
      input_contract: "item_001 完成后的 task_spec.json + README.md 当前版本"
      output_contract: "README.md 的 Five Task Templates 表格更新为包含实际项目 item_id 和 objective"
      acceptance_tests:
        - pack_module_002
      context_budget: "preferred 40k, max 100k"
      allowed_files:
        - docs/stage0/README.md
        - docs/stage0/tasks/task-003-doc-update/run_log.md
        - docs/stage0/tasks/task-003-doc-update/events.jsonl
        - docs/stage0/tasks/task-003-doc-update/completion.json
        - docs/stage0/project_board.md
      forbidden_files:
        - src/
        - tests/
        - runtime/
        - .runtime/
        - .git/
      artifact_refs: []
      retry_count: 0
      failure_code: ""
      escalation_policy: "ask human for clarification"

    - item_id: item_004
      title: "Validate approval_request template in task-004"
      type: test_case
      status: done
      priority: P2
      owner_agent: ""
      dependencies: []
      input_contract: "task-004-config-rule/run_log.md 中的 approval_request 引用模板"
      output_contract: "run_log.md 中 approval_request 模板字段已填写（模拟数据），记录审批流程是否可读"
      acceptance_tests:
        - pack_module_002
      context_budget: "preferred 40k, max 100k"
      allowed_files:
        - docs/stage0/events.jsonl
        - docs/stage0/tasks/task-004-config-rule/events.jsonl
        - docs/stage0/tasks/task-004-config-rule/run_log.md
        - docs/stage0/tasks/task-004-config-rule/completion.json
        - docs/stage0/project_board.md
        - docs/stage0/project_dependency_graph.md
        - docs/stage0/batch_digest.md
        - docs/stage0/README.md
      forbidden_files:
        - src/
        - tests/
        - runtime/
        - .runtime/
        - .git/
      artifact_refs: []
      retry_count: 0
      failure_code: ""
      escalation_policy: "block and ask human"

    - item_id: item_005
      title: "Deliberate failure to validate Advisor Protocol"
      type: module
      status: done
      priority: P1
      owner_agent: ""
      dependencies:
        - item_002
      dependency_type: soft_dependency
      input_contract: "item_002 的 bugfix 流程记录 + task-005-failure-fix-loop 模板"
      output_contract: "至少一次 failed_retryable，Advisor Protocol 调用记录完整，Fix Loop 结果记录在 run_log.md"
      acceptance_tests:
        - pack_module_002
      context_budget: "preferred 80k, max 200k"
      allowed_files:
        - docs/stage0/events.jsonl
        - docs/stage0/tasks/task-005-failure-fix-loop/events.jsonl
        - docs/stage0/tasks/task-005-failure-fix-loop/run_log.md
        - docs/stage0/tasks/task-005-failure-fix-loop/handoff_pack.json
        - docs/stage0/tasks/task-005-failure-fix-loop/completion.json
        - docs/stage0/project_board.md
        - docs/stage0/project_dependency_graph.md
        - docs/stage0/batch_digest.md
        - docs/stage0/README.md
      forbidden_files:
        - src/
        - tests/
        - runtime/
        - .runtime/
        - .git/
      artifact_refs: []
      retry_count: 1
      failure_code: "F008_FORMAT_ERROR"
      escalation_policy: "invoke Advisor Protocol, retry once, then record final failure"
```

## Item → Task Directory Mapping

| item_id | task directory | type |
|---------|---------------|------|
| item_001 | task-001-code-small | module |
| item_002 | task-002-bugfix | bug |
| item_003 | task-003-doc-update | doc |
| item_004 | task-004-config-rule | test_case |
| item_005 | task-005-failure-fix-loop | module |

## Task Queue Status Mapping (§6.8)

| Task Queue Status | Project Board Status | Note |
|-------------------|---------------------|------|
| QUEUED | ready | Entered queue, not yet executing |
| TRIAGED | ready | Triaged, not yet executing |
| READY | ready | Executable |
| READY_READONLY | ready | Read-only prep, write not unlocked |
| READY_WRITE | ready | Write conditions met |
| RUNNING | running | At least one node processing |
| WAITING_APPROVAL | blocked (approval) | Awaiting human approval |
| PAUSED_BUDGET | blocked (budget) | Budget paused |
| WAITING_DEPENDENCY | blocked (dependency) | Waiting upstream |
| BLOCKED | blocked (generic) | Generic block |
| BLOCKED_UPSTREAM_FAILED | blocked (upstream_failed) | Upstream failure |
| BLOCKED_APPROVAL | blocked (approval) | Approval block |
| BLOCKED_PROVIDER | blocked (provider) | Model/API unavailable |
| COMPLETED | review | Done, awaiting Final Gate |
| FAILED | failed | Failed, no auto-retry |
| CANCELLED_BY_DEPENDENCY | failed | Cancelled due to dependency failure |

Final Gate mapping: pass → done, pass_with_notes → review, fail → failed
