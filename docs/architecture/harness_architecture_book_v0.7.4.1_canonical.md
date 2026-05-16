# Token-Efficient Agent Harness 架构书 v0.7.4.1-canonical

版本：v0.7.4.1-canonical  
状态：v0.7.4 正式合并版的小补丁；在 v0.7.3 runtime spec 基础上，正式加入 Project Management Plane，并补齐 Stage 1 前置实现缺口。  
用途：Stage 0 启动前的架构参考文档。  

---

## 0. 与 v0.7.3 的关系

v0.7.4-canonical **不是单独的 preview 补充文档**，而是对 v0.7.3 的正式增量合并。

### 0.1 继承 v0.7.3 的内容

以下 v0.7.3 内容继续有效，除非本文明确覆盖：

```text
Harness Kernel
events.jsonl
task state projection
completion.json
checkpoint
handoff_pack
artifact
approval_request
batch_digest
Policy Engine
Sandbox Manager
Model Gateway
Advisor Protocol
Provider Failover
failure_code → fallback_strategy
Stage 0–4 分层
```

### 0.2 v0.7.4 覆盖 v0.7.3 的章节

本文正式覆盖或扩展以下部分：

```text
Stage 0 Runbook
Stage 0 Exit Criteria
Stage 1 MVP 必做列表
设计决定记录
系统总览 flow
任务入口定义
```

### 0.3 v0.7.4 新增内容

```text
Project Management Plane
Project Architect / PM Agent
Project Board
Project Dependency Graph
Module Contract
Test Case Pack
Project-to-Queue Handoff
Project Board ↔ Task Queue 状态映射
Project-level Stage 0
```

---

## 1. 一句话定义

Token-Efficient Agent Harness 不是一个单一 agent，而是一套面向项目级开发的：

```text
项目管理层
+ 批量任务运行时
+ 沙盒执行器
+ 顾问策略
+ 质量门
+ 经验沉淀系统
```

核心目标：

> 聪明模型负责项目拆分、接口设计、测试设计、风险判断和关键纠偏；便宜模型和本地执行器负责小模块执行；Harness 负责队列、沙盒、依赖、审批、恢复、评分和日志；人只处理高层目标、审批和最终验收。

---

## 2. v0.7.4 的核心判断

v0.7.3 解决的是 **Task Runtime**：

> 一个任务进入系统后，如何被可靠执行、隔离、验证、审批、恢复和复盘。

v0.7.4 新增的是 **Project Management Plane**：

> 一个项目如何先被聪明 AI 拆成可执行、可测试、可排队的小模块。

新的总入口：

```text
Project Brief
→ Project Management Plane
→ Project Board / Module Backlog / Test Case Packs
→ Batch Task Queue
→ Harness Runtime
→ Sandboxed Workers
→ Verifier / Reviewer / Final Gate
→ Batch Digest / Retrospective / Skill Library
```

---

## 3. 9 条核心原则

1. **项目先拆成模块，模块再进入任务队列。**  
   不要把整个项目直接丢给一个 agent。

2. **Project Board 是项目状态源，Task Queue 是执行队列。**  
   Project Board 记录项目事实；Task Queue 只执行 ready items。

3. **聪明模型负责项目抽象，便宜模型负责局部执行。**  
   Project Architect 使用 strong_planner 级模型；Builder 只执行明确的小任务。

4. **模块必须有 Context Budget Contract。**  
   模块必须小到能在单个 active context 内完成。完整项目状态不塞进模型上下文。

5. **所有写入必须进入 sandbox / worktree。**  
   主仓库默认只读。Build 可以并行，Merge 必须收口。

6. **Advisor 是短纠偏，不接管执行。**  
   Advisor 给 diagnosis、recommended_action、do_not_do，不替代 Builder 写完整实现。

7. **质量由测试、规则、review 和 final gate 共同判断。**  
   抽卡不是随机采样，必须靠 scoring、artifact、baseline 和 verifier 支撑。

8. **无人值守必须可审批、可恢复、可解释。**  
   任何高风险动作进入 Approval Broker；早上通过 Batch Digest 接管。

9. **Stage 0 先手动模拟，不急着做完整平台。**  
   先用 Markdown / JSON project board 跑 5 个真实任务，验证 schema 和流程。

---

## 4. 分层架构

### 4.1 Project Management Plane

负责把项目目标拆成可执行模块。

组件：

```text
Project Brief
Project Architect / PM Agent
Project Board
Module Contract Registry
Test Case Pack Registry
Project Dependency Graph
Project-to-Queue Handoff
```

职责：

```text
需求澄清
模块划分
接口定义
测试用例设计
模块依赖管理
任务优先级排序
集成策略设计
回滚策略设计
```

---

### 4.2 Batch Intake / Queue System

负责把 ready 的 project items 送入执行队列。

组件：

```text
Batch Task Inbox
Batch Intake
Cross-task Dependency Manager
Priority Queue
Task State Manager
```

职责：

```text
任务排队
优先级处理
跨任务依赖
只读预执行
写入解锁
失败传播
```

---

### 4.3 Control Plane

负责把任务编译成 DAG，并绑定资源。

```text
Intake
Triage
Planner ↔ Architect
DAG Compiler
Resource Binder / Context Broker
Budget Controller
Policy Engine
```

---

### 4.4 Advisor Plane

负责强模型短介入。

```text
preflight
checkpoint
stuck
arbitration
risk_scan
```

---

### 4.5 Execution Plane

负责实际执行。

```text
Sandbox Manager
Model Gateway
Worker Nodes
Dynamic Concurrency Controller
Checkpoint Writer
```

Worker 类型：

```text
Research Worker
Code Scanner
Builder
Doc Writer
Test Writer
Refactor Worker
Config Worker
```

注意：**Merge Worker 不属于 Execution Plane。**  
它属于 Verification / Integration Plane，因为它处理 fragment / patch 冲突合并，而不是普通执行节点。

---

### 4.6 Verification / Integration Plane

负责质量控制、集成和最终准入。

```text
Verifier
Reviewer
Scoring Engine
Fragment Integrator
Merge Worker
Final Gate
Artifact Gate
```

---

### 4.7 Memory / Optimization Plane

负责沉淀经验。

```text
Run Logs
Eval Records
Retrospective
Skill Extractor
Skill Library
Baseline Store
Routing Optimizer
Policy Candidate Store
```

---

## 5. 完整 Flow

### 5.1 项目级 Flow

```text
Project Brief
→ Project Architect / PM Agent
→ Project Board
→ Module Contracts
→ Test Case Packs
→ Project Dependency Graph
→ Ready Item Selection
→ Project-to-Queue Handoff
→ Batch Task Queue
```

### 5.2 任务级 Flow

```text
Task Queue
→ Intake
→ Triage
→ Planner ↔ Architect
→ Advisor Preflight
→ DAG Compiler
→ Resource Binder / Context Broker
→ Budget Controller
→ Scheduler
→ Sandbox Manager
→ Worker Nodes
→ Verifier
→ Reviewer
→ Final Gate
→ Retrospective
→ Batch Digest
```

### 5.3 执行中异常 Flow

```text
Worker Running
→ Checkpoint
→ Drift / Failure / Tool Error / Scope Violation
→ Fallback Engine
→ Advisor or Task Split or Tool Switch or Human Approval
→ Resume / Retry / Block / Cancel
```

### 5.4 抽卡 Flow

```text
Plan Sampling / Review Sampling / Build Sampling
→ Candidate Sandboxes
→ Scoring Engine
→ Fragment Extractor
→ Conflict Graph
→ Merge Worker
→ Integration Sandbox
→ Verifier
→ Reviewer
→ Final Gate
```

---

## 6. Project Management Plane 详细规范

### 6.1 Project Brief

```yaml
project_brief:
  project_id:
  title:
  objective:
  background:
  non_goals:
  constraints:
  success_criteria:
  delivery_artifacts:
  risk_level:
  preferred_stage:
  human_owner:
```

---

### 6.2 Project Architect / PM Agent

Project Architect 是项目级拆解节点，必须显式声明模型层级。

```yaml
project_architect_node:
  node_id: project_architect
  role: Project Architect / PM Agent
  model_tier: strong_planner
  tools:
    - project_board_read
    - project_board_write
    - artifact_read
    - module_contract_write
    - test_case_pack_write
    - project_dependency_graph_write
  write_access:
    - project_board
    - module_contracts/
    - test_case_packs/
    - project_dependency_graph.*
  forbidden_write_access:
    - src/
    - tests/
    - runtime/
    - .runtime/
    - runs/
    - .git/
  outputs:
    - project_architecture_plan
    - module_contracts
    - test_case_packs
    - project_dependency_graph
```

写权限说明：

```text
Project Architect 可以写项目管理产物：
project_board、module_contracts/、test_case_packs/、project_dependency_graph.*。

Project Architect 不能写代码、测试实现、runtime 状态目录或 git 元数据。
代码和测试文件只能由进入 Task Queue 后的 Builder / Test Writer 在 sandbox 中修改。
```

职责：

```text
把 Project Brief 拆成 Module Backlog
给每个模块定义 input/output contract
给每个模块定义 required tests
决定哪些模块可并行
决定哪些模块需要先只读分析
决定哪些失败应 rollback
决定何时进入 integration phase
```

输出：

```yaml
project_architecture_plan:
  project_id:
  module_backlog:
  dependency_graph:
  integration_plan:
  test_strategy:
  rollback_strategy:
  risk_register:
  queue_handoff_policy:
```

与已有节点的关系：

```text
Project Architect：项目级拆分
Planner / Architect：任务级拆分
Builder：节点级执行
```

---

### 6.3 Project Board

Project Board 是项目状态源，不是执行队列。

```yaml
project_board:
  project_id:
  board_version:
  status:
  items:
    - item_id:
      title:
      type: requirement | module | test_case | bug | doc | integration
      status: todo | ready | running | blocked | review | done | failed
      priority: P0 | P1 | P2 | P3
      owner_agent:
      dependencies:
      input_contract:
      output_contract:
      acceptance_tests:
      context_budget:
      allowed_files:
      forbidden_files:
      artifact_refs:
      retry_count:
      failure_code:
      escalation_policy:
  updated_at:
```

Project Board item 状态含义：

```text
todo：尚未准备进入队列
ready：可进入 Batch Task Queue
running：已有任务在执行
blocked：被依赖、审批、provider 或失败阻塞
review：等待审查或 final gate
done：完成
failed：失败且不自动重试
```

---

### 6.4 Project Dependency Graph

Project Dependency Graph 被 Project Architect 生成，由 Project Board、Cross-task Dependency Manager、Project-to-Queue Handoff 消费。

```yaml
project_dependency_graph:
  graph_id:
  project_id:
  graph_version:
  nodes:
    - node_id:
      item_id:
      node_type: requirement | module | test_case | bug | doc | integration
      status: todo | ready | running | blocked | review | done | failed
      artifact_refs:
      risk_level:
  edges:
    - edge_id:
      from_node:
      to_node:
      dependency_type:
        - hard_dependency
        - artifact_dependency
        - soft_dependency
        - approval_dependency
      required_artifacts:
        - artifact_id:
          artifact_type:
          required_status: draft | verified | accepted
      downstream_policy:
        on_upstream_success: start
        on_upstream_fail: block | cancel | run_readonly_only | use_previous_artifact
        on_upstream_partial: allow_prefetch | wait | ask_approval
  created_at:
  updated_at:
```

规则：

```text
hard_dependency：上游未完成时，下游不能进入 write phase。
artifact_dependency：由 Artifact Gate 判断解锁级别。
soft_dependency：允许只读预执行，但 final integration 前必须满足。
approval_dependency：必须等待 Approval Broker 决策。
```

---

### 6.5 Module Contract

每个模块必须有明确输入、输出、测试和上下文预算。

```yaml
module_contract:
  module_id:
  title:
  objective:
  input_interface:
  output_interface:
  acceptance_criteria:
  required_tests:
  context_budget:
    preferred_tokens:
    max_tokens:
  dependencies:
  allowed_files:
  forbidden_files:
  risk_level:
  integration_points:
  rollback_policy:
```

Context Budget Contract：

```yaml
module_context_contract:
  max_active_context_tokens: 200000
  preferred_context_tokens: 80000
  input_contract_required: true
  output_contract_required: true
  test_case_required: true
  artifact_refs_only: true
```

说明：`200000` 是默认目标，不是硬编码规则。真正原则是模块必须小到能在单个 active context 内完成。

---

### 6.6 Test Case Pack

```yaml
test_case_pack:
  pack_id:
  module_id:
  required_tests:
    - test_id:
      description:
      input:
      expected_output:
      verification_method:
  optional_tests:
  regression_tests:
  pass_threshold:
```

测试设计原则：

```text
每个 module 至少有一个 required test
integration module 必须有 integration test
config/rule module 必须有 negative test 或 safety check
bugfix module 必须有 reproduction path
doc module 必须有 review checklist
```

---

### 6.7 Project-to-Queue Handoff

```yaml
project_to_queue_handoff:
  project_id:
  selected_items:
  batch_id:
  scheduling_policy:
  max_parallel_modules:
  write_conflict_policy:
  approval_policy:
```

Handoff 规则：

```text
只有 status=ready 的 project item 可以进入 Task Queue
依赖未满足的 item 不能进入 write phase
可先进入 readonly prefetch
Project Board 更新必须由 Kernel 记录事件
Task 完成后必须回写 Project Board
```

---

### 6.8 Project Board ↔ Task Queue 状态映射

Project Board 是项目事实源，Task Queue 是执行状态源。Task 状态变化必须映射回 Project Board item。

```yaml
project_board_task_status_mapping:
  QUEUED:
    project_status: ready
    note: "已进入队列但尚未执行，项目项仍视为 ready。"

  TRIAGED:
    project_status: ready
    note: "已完成任务分级，尚未进入执行。"

  READY:
    project_status: ready
    note: "可执行。"

  READY_READONLY:
    project_status: ready
    note: "只读准备可执行，写入尚未解锁。"

  READY_WRITE:
    project_status: ready
    note: "写入条件满足，可进入执行。"

  RUNNING:
    project_status: running
    note: "至少一个执行节点正在处理该项目项。"

  WAITING_APPROVAL:
    project_status: blocked
    blocked_reason: approval
    note: "等待人工审批，不能视为正常 running。"

  PAUSED_BUDGET:
    project_status: blocked
    blocked_reason: budget
    note: "预算暂停。"

  WAITING_DEPENDENCY:
    project_status: blocked
    blocked_reason: dependency
    note: "等待上游依赖。"

  BLOCKED:
    project_status: blocked
    blocked_reason: generic
    note: "通用阻塞。"

  BLOCKED_UPSTREAM_FAILED:
    project_status: blocked
    blocked_reason: upstream_failed
    note: "上游任务失败导致阻塞。"

  BLOCKED_APPROVAL:
    project_status: blocked
    blocked_reason: approval
    note: "审批阻塞。"

  BLOCKED_PROVIDER:
    project_status: blocked
    blocked_reason: provider
    note: "模型或 API provider 不可用。"

  COMPLETED:
    project_status: review
    note: "任务执行完成，等待 Final Gate 或项目级 review。"

  FAILED:
    project_status: failed
    note: "任务失败且不自动重试。"

  CANCELLED_BY_DEPENDENCY:
    project_status: failed
    note: "依赖失败导致取消。"
```

Final Gate 后的映射：

```yaml
final_gate_to_project_board_mapping:
  pass:
    project_status: done
  pass_with_notes:
    project_status: review
  fail:
    project_status: failed
```

---

## 7. Runtime 核心协议摘要

本节继承 v0.7.3 canonical runtime contract，只列摘要。

### 7.1 Node Contract

```yaml
node_id:
role:
goal:
model_tier:
tools:
sandbox:
inputs:
outputs:
success_condition:
failure_condition:
fallback:
metrics:
pass_threshold:
selection_score_formula:
hard_fail_conditions:
idempotency:
  level: safe | conditional | unsafe
```

---

### 7.2 events.jsonl

```json
{
  "event_id": "evt_20260508_000001",
  "schema_version": "event.v1",
  "event_type": "node_completed",
  "timestamp": "2026-05-08T10:15:30Z",
  "producer": {
    "component_id": "builder_01",
    "component_type": "node_runner"
  },
  "correlation": {
    "batch_id": "batch_20260508_night",
    "task_id": "task_003",
    "node_id": "builder",
    "run_id": "run_003_a",
    "dag_version": "dag_v2",
    "sandbox_id": "sbx_task003_builder"
  },
  "severity": "info",
  "payload": {},
  "idempotency_key": "task_003:builder:completed:v1",
  "parent_event_id": "evt_20260508_000000"
}
```

### 7.2.1 项目级事件类型

v0.7.4.1 新增项目级事件类型，用于 Project Board、Project Dependency Graph 和 Project-to-Queue Handoff 的 Kernel 记录。

最低 Stage 1 必需事件：

```text
project_board_item_updated
project_to_queue_handoff_created
project_dependency_resolved
project_item_state_changed
```

完整建议枚举：

```text
project_created
project_brief_updated
project_board_created
project_board_item_updated
project_item_state_changed
project_dependency_graph_created
project_dependency_graph_updated
project_dependency_resolved
project_to_queue_handoff_created
module_contract_created
module_contract_updated
test_case_pack_created
test_case_pack_updated
```

项目级事件 payload 建议：

```yaml
project_event_payload:
  project_id:
  board_version:
  item_id:
  previous_status:
  new_status:
  dependency_edge_id:
  artifact_refs:
  handoff_id:
  reason:
```

规则：

```text
Project Board 的任何状态变化必须写入 project_item_state_changed。
Project Board item 的字段更新必须写入 project_board_item_updated。
Project-to-Queue Handoff 生成时必须写入 project_to_queue_handoff_created。
Project Dependency Graph 的依赖被满足时必须写入 project_dependency_resolved。
这些事件和普通 task/node 事件共享同一个 events.jsonl，但 correlation 可以只包含 project_id，不要求必须包含 task_id。
```

---

### 7.3 completion.json

```json
{
  "node_id": "builder",
  "task_id": "task_003",
  "status": "completed",
  "exit_code": 0,
  "completion_type": "success",
  "handoff_pack_ref": "handoff_pack.json",
  "artifact_refs": [
    {
      "artifact_id": "patch_001",
      "path": "artifacts/patch.diff",
      "sha256": "..."
    }
  ],
  "verifier_status": "pass",
  "write_claims_released": true,
  "retry_count": 0,
  "finished_at": "2026-05-08T10:15:30Z"
}
```

---

### 7.4 checkpoint

```json
{
  "checkpoint_id": "ckpt_004",
  "task_id": "task_001",
  "node_id": "builder",
  "dag_version": "dag_v1",
  "sandbox_id": "sbx_task001_builder",
  "status": "running",
  "input_hash": "sha256:...",
  "current_step": "editing parser tests",
  "completed_steps": [
    "read target files",
    "created initial patch"
  ],
  "pending_steps": [
    "run tests",
    "fix failures"
  ],
  "artifact_refs": [
    {
      "type": "partial_patch",
      "path": "artifacts/partial.patch",
      "sha256": "..."
    }
  ],
  "model_call_refs": [],
  "tool_call_refs": [],
  "resumable": true,
  "resume_strategy": "resume_in_same_sandbox",
  "created_at": "2026-05-08T10:12:00Z"
}
```

---

### 7.5 handoff_pack

```yaml
handoff_pack:
  structured_fields:
  summary:
  evidence_refs:
  full_artifact_refs:
```

默认传：

```text
structured_fields
summary
evidence_refs
```

默认不传 full artifact 正文。

---

### 7.6 artifact

```yaml
artifact:
  artifact_id:
  artifact_type: api_spec | patch | test_report | design_doc | config_plan
  producer_task_id:
  producer_node_id:
  version:
  content_hash:
  created_at:
  status:
    - draft
    - verified
    - accepted
    - rejected
    - superseded
  schema_version:
  verifier_score:
  final_gate_status:
  refs:
```

---

### 7.7 approval_request

```yaml
approval_request:
  approval_id:
  task_id:
  risk_level:
  requested_action:
    - modify_files
    - delete_files
    - run_command
    - submit_pr
    - use_paid_api
    - access_external_service
  summary:
  reason:
  affected_files:
  diff_preview:
  cost_estimate:
  risk_notes:
  options:
    - approve
    - reject
    - approve_readonly_only
    - approve_with_constraints
    - defer
  timeout_policy:
  created_at:
  expires_at:
```

---

### 7.8 batch_digest

```yaml
batch_digest:
  batch_id:
  overnight_summary:
  completed_tasks:
  blocked_or_waiting_approval:
  failed_tasks:
  risk_cost_report:
  recommended_actions:
```

Digest 是无人值守系统的主界面。

---

## 8. Policy Engine 摘要

### 8.1 规则字段语义

```text
layer：全局规则层级。不同 layer 永远按层级顺序比较。
priority：仅在同一 layer 内排序，永不跨 layer 比较。
severity：同一 layer 内冲突处理使用。
```

Layer 顺序：

```text
Safety > Approval > Budget > Dependency > Quality > Optimization
```

### 8.2 Canonical when 条件格式

```yaml
when:
  field: value
```

```yaml
when:
  field:
    in: [...]
```

```yaml
when:
  budget_usage:
    gte: 0.85
```

合法操作符：

```text
eq
neq
in
not_in
gt
gte
lt
lte
exists
missing
```

---

## 9. Stage 分层

### Stage 0：Manual Project Simulation

目标：不用 runtime，手动验证 Project Board + Task Runtime schema。

新增要求：

```text
Stage 0 的 5 个任务必须从同一个 project_board.md / project_board.json 派生。
```

必须产出：

```text
project_brief.md
project_board.md 或 project_board.json
project_dependency_graph.md 或 project_dependency_graph.json
module_contracts/
test_case_packs/
task_spec.json
events.jsonl 草稿
handoff_pack.json
completion.json
run_log.md
batch_digest.md
retrospective.md
```

退出标准：

```text
1. 完成一个小型 Project Board
2. 至少拆出 5 个 project items
3. 至少定义一个 Project Dependency Graph
4. 至少 3 个 items 进入 task queue 模拟
5. 手动跑完 5 个真实任务
6. 最近 3 个任务不再修改核心 schema
7. 至少 2 次使用 Advisor Protocol
8. 至少 1 次失败进入 Fix Loop
9. 5 个任务都验证 Project Board 状态回写
10. Batch Digest 能让人明确判断下一步动作
```

### Stage 1：MVP Batch Runner

必须有：

```text
Harness Kernel
events.jsonl
Task Queue
Node Completion Protocol
Sandbox Manager
Model Gateway
Advisor Protocol
Approval Broker
Policy Engine
Batch Digest
Run Log / Checkpoint
Basic Verifier
Project Board v1
Project Dependency Graph v1
Project-to-Queue Handoff v1
Project Board ↔ Task Queue 状态映射
Project-level Event Types
```

不做：

```text
完整 Web UI
复杂动态 DAG
自动 routing optimizer
持续 trajectory monitor
fragment-level cherry-pick
复杂 build sampling
```

### Stage 2：Quality Runtime

```text
Scoring Engine
Plan Sampling
Review Sampling
Basic Baseline Comparison
Trajectory Monitor v1
Artifact Gate v1
Cross-task Dependency v1
Test Case Pack execution
```

### Stage 3：Optimization Runtime

```text
Fragment Integrator
Merge Worker
Routing Optimizer
Skill Extractor
Canary Policy Deployment
Drift Calibration Loop
Dynamic Concurrency Controller
Project Board CLI / TUI
```

### Stage 4：Autonomous Harness Platform

```text
dynamic DAG mutation
multi-task artifact graph
advanced approval UI
multi-sandbox orchestration
routing canary
long-term skill library
policy rollback
local web dashboard
```

---

## 10. Stage 0 Runbook v0.7.4

### 10.1 目标

用一个真实小项目验证：

```text
Project Board 是否能承载项目状态
Project Dependency Graph 是否能表达依赖
Module Contract 是否能约束任务边界
Test Case Pack 是否能驱动验收
Project-to-Queue Handoff 是否清楚
Task Runtime schema 是否够用
Batch Digest 是否能作为早晨主界面
```

### 10.2 推荐目录

```text
stage0/
  project_brief.md
  project_board.md
  project_dependency_graph.md
  module_contracts/
    module_001.md
    module_002.md
  test_case_packs/
    module_001_tests.md
    module_002_tests.md
  tasks/
    task-001-code-small/
      task_spec.json
      events.jsonl
      handoff_pack.json
      completion.json
      run_log.md
      retrospective.md
    task-002-bugfix/
    task-003-doc-update/
    task-004-config-rule/
    task-005-failure-fix-loop/
  batch_digest.md
```

### 10.3 五个任务模板

#### Task 1：模块级代码小改动

```yaml
task_template:
  task_id: stage0_task_001
  source_project_item: module_001
  type: code_small_change
  objective: "实现一个低风险模块功能"
  required_inputs:
    - module_contract
    - test_case_pack
  success_criteria:
    - 代码变更范围清晰
    - required tests 通过
    - completion.json 合规
    - Project Board 状态回写
```

#### Task 2：模块级 bugfix

```yaml
task_template:
  task_id: stage0_task_002
  source_project_item: bug_001
  type: bugfix
  objective: "修复一个明确可复现的小 bug"
  required_inputs:
    - reproduction_path
    - module_contract
  success_criteria:
    - bug 可复现
    - 修复后验证通过
    - failure_code 被正确记录
    - run_log 能解释修复原因
    - Project Board 状态回写
```

#### Task 3：文档 / Source 更新

```yaml
task_template:
  task_id: stage0_task_003
  source_project_item: doc_001
  type: doc_update
  objective: "更新架构或 source 文档"
  success_criteria:
    - 文档结构清晰
    - 未破坏既有约束
    - reviewer 能从 handoff_pack 判断是否完成
    - digest 能展示改动摘要
    - Project Board 状态回写
```

#### Task 4：配置 / 规则类任务

```yaml
task_template:
  task_id: stage0_task_004
  source_project_item: rule_001
  type: config_or_rule_change
  objective: "修改低风险配置或规则文件"
  success_criteria:
    - allowed_files / forbidden_files 明确
    - approval_request 可读
    - 未授权动作没有执行
    - Project Board 记录审批状态
    - Project Board 状态回写
```

#### Task 5：失败后修复任务

```yaml
task_template:
  task_id: stage0_task_005
  source_project_item: failure_case_001
  type: failure_then_fix_loop
  objective: "故意制造一次失败，走 Advisor / Fix Loop"
  success_criteria:
    - 至少产生一次 failed_retryable
    - Advisor Protocol 至少调用一次
    - Fix Loop 后成功或给出合理失败解释
    - failure_code 和 advisor_response 被完整记录
    - Project Board 状态回写
```

---

## 11. 当前实现路线

### 不要先做

```text
完整 Web 项目管理
完全自动 routing optimizer
复杂多 builder merge
长期自进化
远程 SaaS agent 直连 GitHub 写入
```

### 应该先做

```text
project_board.md
project_dependency_graph.md
module_contract.md
test_case_pack.md
手动 Stage 0 跑 5 个任务
验证 digest 是否有用
验证 handoff_pack 是否足够
验证 Advisor 是否真的能短纠偏
```

---

## 12. 设计决定记录补充

### 12.1 v0.7.4 正式 supersede v0.7.3 的部分章节

决定：v0.7.4-canonical 正式覆盖 v0.7.3 的 Stage 0、Stage 1、系统 flow、设计决定记录相关部分。  
理由：Project Management Plane 改变了系统入口和 Stage 0 方式，不能再作为 preview 附录存在。

### 12.2 新增 Project Management Plane

决定：在 Batch Task Queue 之上新增 Project Management Plane。  
理由：v0.7.3 解决任务执行，但项目开发需要需求、模块、接口、测试、依赖和项目状态管理。

### 12.3 Project Board 是状态源，Task Queue 是执行队列

决定：Project Board 记录项目事实；Task Queue 只执行 ready items。  
理由：如果直接把项目拆成散任务入队，系统会丢失项目级上下文和依赖关系。

### 12.4 Project Dependency Graph 必须 schema 化

决定：Project Dependency Graph 有独立 schema，包含 nodes、edges、dependency_type 和 downstream_policy。  
理由：Cross-task Dependency Manager 需要稳定输入，不能从自然语言依赖描述中猜。

### 12.5 Project Board 与 Task Queue 必须有状态映射表

决定：Task 状态变化必须明确回写 Project Board。  
理由：Project Board 是状态源，如果映射不明确，Stage 1 Kernel 会产生不一致状态。

### 12.6 Project Architect 显式使用 strong_planner

决定：Project Architect / PM Agent 的 `model_tier` 是 `strong_planner`。  
理由：项目级抽象决定项目质量，不应交给 cheap executor。

### 12.7 Project Architect 与 Planner / Architect 分层

决定：Project Architect 做项目级模块拆分；Planner / Architect 做任务级 DAG 拆分。  
理由：模块边界和项目质量由上层设计决定，不应交给 Builder 或任务级 Planner 临时决定。

### 12.8 Merge Worker 只属于 Verification / Integration Plane

决定：Merge Worker 从 Execution Plane Worker 类型中移除，只保留在 Verification / Integration Plane。  
理由：Merge Worker 负责 fragment / patch 冲突解决，属于集成层，不是普通执行层。

### 12.9 模块必须有 input/output/test/context contract

决定：每个 module_contract 必须包含输入接口、输出接口、验收标准、测试包和 context budget。  
理由：只有模块边界明确，小 agent 才能安全并行执行。

### 12.10 200k 是默认 context budget，不是硬编码规则

决定：`max_active_context_tokens: 200000` 作为默认建议值。  
理由：不同模型上下文不同。真正原则是模块必须小到能在单个 active context 内完成。

### 12.11 Stage 0 必须从 Project Board 派生任务

决定：Stage 0 的 5 个任务不再是孤立任务，而是从同一个 project_board 派生。  
理由：这样才能验证项目级 workflow，而不是只验证 task runtime。

### 12.12 Stage 0 五个任务都必须验证 Project Board 状态回写

决定：5 个任务模板全部加入 Project Board 状态回写验收。  
理由：Project Board 是 v0.7.4 核心新增层，必须在 Stage 0 全面验证。

### 12.13 Project Architect 写权限只限项目管理产物

决定：Project Architect 可以写 `project_board`、`module_contracts/`、`test_case_packs/`、`project_dependency_graph.*`，但不能写 `src/`、`tests/`、`.runtime/`、`runs/` 或 `.git/`。  
理由：Project Architect 负责项目级拆分和管理产物生成，不应绕过 Task Queue、Sandbox Manager 和 Builder 直接修改代码或运行时状态。

### 12.14 项目级事件类型进入 Stage 1 Preflight

决定：Stage 1 前必须补齐项目级事件类型，包括 `project_board_item_updated`、`project_to_queue_handoff_created`、`project_dependency_resolved`、`project_item_state_changed`。  
理由：Project Board 是项目状态源，任何项目状态变化都必须被 Kernel 记录和恢复，否则 Project Board 与 Task Queue 会产生状态漂移。

### 12.15 Web UI 后置

决定：项目管理先用 Markdown / JSON，Web UI 放到 Stage 3/4。  
理由：当前最值钱的是协议闭环，不是界面。过早做 UI 会拖慢 runtime 验证。

---

## 13. 结论

v0.7.4 的核心变化是：

> v0.7.3 的 runtime 前面，正式增加 Project Management Plane。

新的系统边界：

```text
Project Management Plane：定义项目、模块、接口、测试、状态、依赖图
Batch Runtime：排队、沙盒、执行、审批、恢复
Agent Workers：编码、测试、git、文档
Advisor / Project Architect：控制项目质量和回滚
Memory Plane：沉淀日志、skill、baseline 和 routing 经验
```

下一步：

```text
进入 Stage 0。
先用 Markdown / JSON project board 手动跑 5 个真实任务。
不要继续纸面扩展。
用真实任务修正 schema。
```
