# Batch Digest

Source: harness_architecture_book_v0.7.4.1-canonical §7.8

Digest is the primary interface for unattended operation.
Human reviews this each morning to decide next actions.

```yaml
batch_digest:
  batch_id: batch_20260515_stage0
  overnight_summary: >
    Stage 0 手动模拟四个任务已完成：item_001 模板补全、item_002 schema bugfix（均通过 Final Gate）、item_003 README 更新（Final Gate pass）、item_004 approval_request 模板验证（review 中）。
    发现并修复了 task_spec.json 缺少 project_id 字段的 schema inconsistency。
    Task 002、003、004 均发现 allowed_files 不完整，已记录为 scope correction（第三次出现）。
    item_003 Final Gate 通过，状态 review → done。
    item_004 完成：approval_request 模板已填写，decision 为 pending，等待人工审批。
    item_004 Final Gate 通过：approval_request 模板字段完整、语义清楚、decision 保持 pending。
    item_005 完成：故意制造 F008_FORMAT_ERROR 失败，2 次 Advisor Protocol 调用（Preflight + Correction），Fix Loop 修复后通过 verifier。item_005 进入 review。
  completed_tasks:
    - task_id: stage0_task_001
      project_item_id: item_001
      type: code_small_change
      status: completed
      summary: "填写 5 个 task_spec.json 的 objective、risk_level、allowed_files、forbidden_files 字段"
      artifacts_produced:
        - task_spec_001
        - task_spec_002
        - task_spec_003
        - task_spec_004
        - task_spec_005
      duration_estimate: "5 min (manual sim)"
      board_writeback_status: done
      final_gate_result: pass

    - task_id: stage0_task_002
      project_item_id: item_002
      type: bugfix
      status: completed
      summary: "修复 task_spec.json 缺少 project_id 字段，记录 scope correction"
      artifacts_produced:
        - task_spec_001_patched
        - task_spec_002_patched
        - task_spec_003_patched
        - task_spec_004_patched
        - task_spec_005_patched
      duration_estimate: "5 min (manual sim)"
      board_writeback_status: done
      final_gate_result: pass

    - task_id: stage0_task_003
      project_item_id: item_003
      type: doc_update
      status: completed
      summary: "更新 README.md：新增 Stage 0 Current Status、Lessons Learned、Responsibility Boundaries 章节"
      artifacts_produced:
        - readme_updated
      duration_estimate: "5 min (manual sim)"
      board_writeback_status: done
      final_gate_result: pass

    - task_id: stage0_task_004
      project_item_id: item_004
      type: config_or_rule_change
      status: completed
      summary: "验证 approval_request 模板：填写模拟数据，记录审批流程，生成 waiting_approval 状态"
      artifacts_produced:
        - approval_request_template_filled
      duration_estimate: "5 min (manual sim)"
      board_writeback_status: done
      final_gate_result: pass

    - task_id: stage0_task_005
      project_item_id: item_005
      type: failure_then_fix_loop
      status: completed
      summary: "故意制造 handoff_pack 不完整失败（F008_FORMAT_ERROR），2 次 Advisor Protocol 调用（Preflight + Correction），Fix Loop 修复后通过"
      artifacts_produced:
        - failure_loop_trace
        - advisor_protocol_record
      duration_estimate: "10 min (manual sim)"
      board_writeback_status: done
      final_gate_result: pass

  blocked_or_waiting_approval: []

  failed_tasks: []

  risk_cost_report: >
    三个任务均为 low risk，无 token 消耗（手动模拟）。
    发现 1 个 schema inconsistency（task_spec.json 缺少 project_id），已修复。
    发现 4 个 scope correction（item_002-005 的 allowed_files 不完整），已修复并记录。

  recommended_actions:
    - action_id: action_001
      priority: resolved
      action_type: review
      target_task_id: stage0_task_003
      description: "Final Gate for item_003: 验证 README.md 更新是否满足 output_contract [RESOLVED: item_003 Final Gate pass, 2026-05-15T21:35:00]"
      reason: "item_003 已进入 done"

    - action_id: action_002
      priority: resolved
      action_type: review
      target_task_id: stage0_task_004
      description: "启动 item_004：验证 approval_request 模板 [RESOLVED: task-004 completed, item_004 → review, 2026-05-15T21:45:00]"
      reason: "item_004 已进入 review"

    - action_id: action_003
      priority: resolved
      action_type: review
      target_task_id: stage0_task_005
      description: "启动 item_005：故意失败验证 Advisor Protocol [RESOLVED: task-005 completed, item_005 → review, 2026-05-15T22:00:00]"
      reason: "item_005 已进入 review"

    - action_id: action_005
      priority: resolved
      action_type: review
      target_task_id: stage0_task_005
      description: "Final Gate for item_005 [RESOLVED: item_005 Final Gate pass, 2026-05-15T22:05:00]"
      reason: "item_005 Final Gate 通过，已进入 done"

    - action_id: action_004
      priority: resolved
      action_type: review
      target_task_id: stage0_task_004
      description: "Review approval_request for item_004: 验证 run_log.md 中的 approval_request 模板是否可读，决定是否通过 Final Gate [RESOLVED: item_004 Final Gate pass, 2026-05-15T21:50:00]"
      reason: "item_004 Final Gate 通过，已进入 done"
```

## Final Gate Record — 2026-05-15T21:20

### item_001 Final Gate

| Check | Result |
|-------|--------|
| 5 task_spec.json objective non-empty | PASS |
| 5 task_spec.json risk_level non-empty | PASS |
| 5 task_spec.json allowed/forbidden_files filled | PASS |
| completion.json status=completed | PASS |
| run_log records schema inconsistency | PASS |
| item_001 status = review | PASS |
| **Decision** | **PASS → done** |

### item_003 Final Gate

| Check | Result |
|-------|--------|
| README.md has Stage 0 Current Status section | PASS |
| README.md has updated Five Task Templates table | PASS |
| README.md has Responsibility Boundaries section | PASS |
| README.md has Lessons Learned section | PASS |
| completion.json status=completed | PASS |
| run_log records scope correction (allowed_files) | PASS |
| item_003 status = review | PASS |
| **Decision** | **PASS → done** |

### item_004 Final Gate

| Check | Result |
|-------|--------|
| approval_request 字段完整 | PASS |
| requested_action = modify_files | PASS |
| decision = pending | PASS |
| risk_level 已填写 | PASS |
| affected_files 已填写 | PASS |
| options 已填写 | PASS |
| timeout_policy 已填写 | PASS |
| run_log 记录 scope correction | PASS |
| completion.json status=completed | PASS |
| item_004 status = review | PASS |
| 未执行真实审批动作 | PASS |
| **Decision** | **PASS → done** |

### item_002 Final Gate

| Check | Result |
|-------|--------|
| 5 task_spec.json have project_id | PASS |
| project_id matches project_board.md | PASS |
| completion.json status=completed | PASS |
| run_log records bug reproduction | PASS |
| run_log records fix applied | PASS |
| run_log records scope correction | PASS |
| item_002 status = review | PASS |
| **Decision** | **PASS → done** |

### item_005 Final Gate

| Check | Result |
|-------|--------|
| node_failed: failed_retryable in events.jsonl | PASS |
| Advisor Preflight requested + response | PASS |
| Advisor Correction requested + response | PASS |
| fix_loop node_started + artifact_produced + node_completed | PASS |
| run_log failure loop trace (2 attempts) | PASS |
| run_log scope correction recorded | PASS |
| handoff_pack structured_fields complete | PASS |
| handoff_pack summary non-empty | PASS |
| handoff_pack evidence_refs non-empty | PASS |
| completion.json status=completed | PASS |
| failure_code=F008_FORMAT_ERROR | PASS |
| failure_subcode=handoff_pack_incomplete | PASS |
| verifier_status=passed_on_retry | PASS |
| retry_count=1 | PASS |
| item_005 status = review | PASS |
| forbidden_files not touched | PASS |
| **Decision** | **PASS → done** |

### Dependency Resolution

| Edge | From | To | Type | Resolution |
|------|------|----|------|-----------|
| edge_001_003 | item_001 (done) | item_003 | hard_dependency | item_003 → ready |
| edge_002_005 | item_002 (done) | item_005 | soft_dependency | item_005 → ready (write phase allowed) |

## Stage 0 Exit Criteria Progress

| # | Criteria | Status |
|---|----------|--------|
| 1 | Complete a small Project Board | DONE |
| 2 | At least 5 project items extracted | DONE (5 items) |
| 3 | At least one Project Dependency Graph defined | DONE (5 nodes, 3 edges) |
| 4 | At least 3 items enter task queue simulation | DONE: item_001, item_002, item_003 all completed. item_005 remains ready but not yet queued. |
| 5 | Manually run through 5 real tasks | 5/5 completed |
| 6 | Last 3 tasks no longer modify core schema | DONE (tasks 3-5 did not modify task_spec.json / project_brief.md core schema) |
| 7 | At least 2 uses of Advisor Protocol | 2/2 DONE: Advisor Preflight + Advisor Correction both recorded in task-005. |
| 8 | At least 1 failure enters Fix Loop | 1/1 DONE (F008_FORMAT_ERROR → fix_loop, retry_count=1) |
| 9 | All 5 tasks validate Project Board status writeback | 5/5 DONE; project items done: 5/5. |
| 10 | Batch Digest enables clear next-step decisions | DONE (action_005: Final Gate for item_005) |
