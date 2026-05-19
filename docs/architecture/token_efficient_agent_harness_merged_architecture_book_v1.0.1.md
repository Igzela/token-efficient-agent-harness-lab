# Token-Efficient Agent Harness 完整合并架构书

版本：merged-architecture-book-v1.0.1  
合并来源：

```text
1. v0.7.4.1-canonical
   Stage 0–4 核心 runtime 架构、schema、协议、Stage 分层和 Stage 0 runbook。

2. v1.2-post-closeout-update
   Stage 0–4 完成后的 post-closeout 状态、Optional Tracks、Cursor harness research、Keep Rate、用户反馈、Context Pack v2 schemas。

3. v1.2-authority-kernel-addendum
   目标 / 状态 / 质量 / 准入 / 记忆 / Orchestrator 权责边界，以及 Kernel 类型判定。

4. v1.3.2-controlled-adaptive-orchestrator
   Governance & Policy Plane、Evaluation Admission Contract、Policy Candidate Lifecycle、CA Maturity Gates、真实模型接入前置条件。
```

合并规则：

```text
冲突时高版本覆盖低版本。
所有已明确定义的 schema 字段完整保留。
Stage 0–4 分层和退出标准来自 v0.7.4.1，不被覆盖。
CA Maturity Gates 来自 v1.3.2，是最高成熟度判定标准。
Optional Track 顺序以 v1.3.2 Section 15 为准。
设计决定记录按版本分组保留。
禁止事项合并去重。
```

---

# 1. 文档定位与版本关系

## 1.1 v0.7.4.1-canonical

`v0.7.4.1-canonical` 是 Stage 0 启动前的历史架构基准。它在 v0.7.3 runtime spec 基础上，正式加入 Project Management Plane，并补齐 Stage 1 前置实现缺口。

它继续作为以下内容的 canonical 来源：

```text
Project Management Plane
Project Brief
Project Architect / PM Agent
Project Board
Project Dependency Graph
Module Contract
Test Case Pack
Project-to-Queue Handoff
Project Board ↔ Task Queue 状态映射
events.jsonl
completion.json
checkpoint
handoff_pack
artifact
approval_request
batch_digest
Node Contract
Policy Engine
Stage 0–4 分层
Stage 0 Runbook
Stage 0 Exit Criteria
```

## 1.2 v1.2-post-closeout-update

`v1.2-post-closeout-update` 是 Stage 0–4 完成后的封版状态说明。它覆盖旧架构书中“下一步进入 Stage 0”的过时结论，新增 post-closeout optional tracks、Cursor harness research、Keep Rate、用户反馈、Context Pack v2 schemas 和真实模型接入前置路线。

它继续作为以下内容的 canonical 来源：

```text
Post-closeout 状态
Optional Tracks 初始定义
Cursor harness research 吸收结果
keep_rate_observation
user_feedback_event
harness_maintenance_issue
advisor_context_pack_v2
model_context_pack_v2
context_retrieval_request
context_retrieval_result
model_harness_profile
forbidden_previous_tools 语义
```

## 1.3 v1.2-authority-kernel-addendum

`v1.2-authority-kernel-addendum` 是 canonical 权责边界补丁。它明确：

```text
Project Brief = 目标来源
Project Board = 状态事实源
Quality Gate = 质量 / 风险评估器
Final Gate = 最高准入函数
Memory / Optimization Plane = 经验沉淀层
Orchestrator = deterministic coordination / state progression controller
```

它还明确当前系统类型：

```text
Orchestrator Kernel
with controlled adaptive-cognitive extensions
```

不是完整 Adaptive Cognitive Kernel。

## 1.4 v1.3.2-controlled-adaptive-orchestrator

`v1.3.2-controlled-adaptive-orchestrator` 是从 Orchestrator Kernel 走向 Controlled Adaptive Orchestrator Kernel 的治理蓝图。

它是以下内容的最高版本来源：

```text
Controlled Adaptive Orchestrator Kernel 定义
Governance & Policy Plane
Evaluation Admission Contract
Policy Candidate Lifecycle
rollback_plan
usage_ledger
error_record
fixture_metadata
CA Maturity Gates CA-0 到 CA-8
Optional Track 顺序
真实模型接入前置条件
```

---

# 2. 系统定位与核心原则

## 2.1 一句话定义

Token-Efficient Agent Harness 不是一个单一 agent，而是一套面向项目级开发的：

```text
项目管理层
+ 批量任务运行时
+ 沙盒执行器
+ 顾问策略
+ 质量门
+ 经验沉淀系统
+ 治理与策略候选层
```

核心目标：

> 聪明模型负责项目拆分、接口设计、测试设计、风险判断和关键纠偏；便宜模型和本地执行器负责小模块执行；Harness 负责队列、沙盒、依赖、审批、恢复、评分和日志；Governance & Policy Plane 负责候选策略的证据准入、审批、回滚和部署边界；人只处理高层目标、审批和最终验收。

## 2.2 当前系统定位

当前系统是：

```text
Orchestrator Kernel
with controlled adaptive-cognitive extensions
```

它不是完整 Adaptive Cognitive Kernel。

当前已经完成：

```text
Stage 0–4
Project closeout
GitHub private repo publishing
CI
Real-World Read-Only Evaluation Track
Real-World Fixture Expansion
Harness Change Evaluation Track
v1.2 authority/kernel addendum
```

当前仍未开始：

```text
真实模型执行任务
真实 sandbox 执行
真实并发 worker
真实多 agent runtime
Web UI
provider failover
自动 PR / merge
Stage 5
```

## 2.3 Controlled Adaptive Orchestrator Kernel 定义

Controlled Adaptive Orchestrator Kernel 指：

```text
一个由 deterministic Orchestrator 保持执行权威，
由 Evaluation / Memory / Governance / Policy Plane 产生候选改进，
并且只通过 offline evaluation、human approval、rollback-ready deployment
吸收 harness-level 改动的系统。
```

关键限制：

```text
adaptive 的对象是 harness policy / config / schema / profile / skill / eval gate。
adaptive 的对象不是 runtime 自治权。
Orchestrator 不重写目标。
Orchestrator 不拥有长期记忆。
Orchestrator 不直接改策略。
模型建议不能直接通过 Final Gate。
策略候选不能绕过固定评估和人工审批。
```

最短定义：

```text
Controlled Adaptive Orchestrator Kernel =
Deterministic Orchestrator Core
+ Evaluation Plane
+ Memory / Optimization Plane
+ Governance & Policy Plane
+ Approval-gated adaptive policy lifecycle
```

## 2.4 原 9 条核心原则

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

## 2.5 Post-closeout 新增原则

```text
10. 不再自动扩展 Stage。Stage 0–4 已完成，后续只能开独立 Track。
11. 真实模型接入必须从 Advisor-only 开始，不允许直接执行任务。
12. 所有 harness 改动必须经过固定 eval suite 的 before/after 对比。
13. Unknown error 默认视为 harness bug，不能静默吞掉。
14. Context Pack 应从“全量塞入”转向“最小上下文 + 显式检索”。
15. Model routing 不能只看 tier，还必须看 model-specific harness profile。
16. Specialist agent 必须先定义 role profile，再允许真实多 agent 执行。
17. Keep Rate 是真实输出长期价值的核心信号，不能只看测试通过率。
18. 用户 post-action feedback 必须作为质量信号进入评估闭环。
19. Harness 维护必须周期性从日志和失败模式中生成维护 issue，而不是只靠人工记忆。
```

## 2.6 合并后的禁止事项

```text
不允许 Orchestrator 自己改目标。
不允许 Orchestrator 写长期记忆。
不允许 runtime 自动部署 routing policy。
不允许 LLM critique 直接通过 Final Gate。
不允许真实模型直接执行任务。
不允许 shadow routing 直接变 active routing。
不允许 skill extraction 自动改 prompt。
不允许 policy candidate 绕过 offline eval。
不允许 unknown_error 驱动 adaptive candidate。
不允许 diagnostic evidence 单独决定 policy adoption。
不允许创建 Stage 5 来规避 post-closeout track gate。
不允许 Project Architect 写 src/、tests/、runtime/、.runtime/、runs/ 或 .git/。
不允许 Worker 直接写主仓库；写入必须进入 sandbox / worktree。
不允许 Merge Worker 被当作普通 Execution Worker。
不允许 task completed 直接变 project item done。
不允许 approval_request decision=pending 时执行审批动作。
不允许 non-canonical failure_code 通过 validator。
不允许 completion without handoff_pack 被标记为完成。
不允许 forbidden_files touched 后继续执行。
不允许 allowed_files incomplete 时进入正常写入阶段。
不允许 unknown error 被普通 retry 吞掉。
不允许真实模型第一次接入时进行文件修改、shell command、sandbox execution、PR 创建、自动 merge、自动 policy adoption 或自动 prompt mutation。
```

---

# 3. 系统总览

## 3.1 七个子系统

原 v0.7.4.1 定义 6 个核心子系统，v1.3.2 在其上新增 Governance & Policy Plane。

```text
1. Project Management Plane
2. Batch Intake / Queue System
3. Control Plane
4. Advisor Plane
5. Execution Plane
6. Verification / Integration Plane
7. Memory / Optimization Plane
8. Governance & Policy Plane
```

用户要求中的“6 个子系统 + Governance & Policy Plane”可理解为：

```text
六个原始运行子系统：
Project Management
Batch Intake / Queue
Control
Advisor
Execution
Verification / Integration

再加：
Memory / Optimization
Governance & Policy
```

其中 Memory / Optimization 是 v0.7 已有 plane，Governance & Policy 是 v1.3.2 新增治理 plane。

## 3.2 完整项目级 Flow

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
→ Harness Runtime
→ Sandboxed Workers
→ Verifier / Reviewer / Final Gate
→ Batch Digest / Retrospective / Skill Library
→ Memory / Optimization Plane
→ Governance & Policy Plane
→ Optional Policy Candidate Lifecycle
```

## 3.3 完整任务级 Flow

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
→ Quality Gate
→ Final Gate
→ Retrospective
→ Batch Digest
```

## 3.4 执行中异常 Flow

```text
Worker Running
→ Checkpoint
→ Drift / Failure / Tool Error / Scope Violation
→ Error Taxonomy Classification
→ Fallback Engine
→ Advisor or Task Split or Tool Switch or Human Approval
→ Resume / Retry / Block / Cancel
→ Evaluation / Memory Record
→ Optional Policy Candidate
```

## 3.5 抽卡 / Sampling Flow

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

## 3.6 Controlled Adaptive Flow

```text
Monitor
  -> collect traces / snapshots / tool errors / usage / feedback
Analyze
  -> cluster failure / detect regression / identify opportunity
Candidate
  -> generate limited policy/config/schema/profile/skill candidate
Evaluate
  -> run fixed fixtures + realistic read-only + user-style mutation + cost checks
Review
  -> Quality Gate + Final Gate + human approval
Deploy
  -> apply harness config only, with rollback plan
Record
  -> write decision and evidence to policy registry
```

---

# 4. 所有 Plane 定义

## 4.1 Project Management Plane

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

失败行为：

```text
Project Brief 不清楚 → blocked / needs_human_clarification
Module Contract 缺失 → item 不能进入 ready
Test Case Pack 缺失 → item 不能进入 write phase
Dependency Graph 不合法 → Project-to-Queue Handoff 阻断
```

## 4.2 Batch Intake / Queue System

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

失败行为：

```text
未满足 hard_dependency → WAITING_DEPENDENCY
未满足 approval_dependency → WAITING_APPROVAL
provider 不可用 → BLOCKED_PROVIDER
上游失败 → BLOCKED_UPSTREAM_FAILED 或 CANCELLED_BY_DEPENDENCY
```

## 4.3 Control Plane

负责把任务编译成 DAG，并绑定资源。

组件：

```text
Intake
Triage
Planner ↔ Architect
DAG Compiler
Resource Binder / Context Broker
Budget Controller
Policy Engine
```

职责：

```text
任务规格读取
风险分级
DAG 拆分
资源绑定
上下文包编排
预算控制
规则裁决
```

失败行为：

```text
Policy Engine block → task blocked
Budget exceeded → PAUSED_BUDGET
Context pack invalid → context_error
DAG cycle detected → fail / requires_human_review
```

## 4.4 Advisor Plane

负责强模型短介入。

允许 call type：

```text
preflight
checkpoint
stuck
arbitration
risk_scan
correction
```

职责：

```text
短纠偏
风险判断
失败归因建议
不接管执行
不写完整实现
```

失败行为：

```text
budget_exceeded → no model call
missing context → structured error response
unknown advisor error → unknown_error / human triage
```

## 4.5 Execution Plane

负责实际执行。

组件：

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

注意：

```text
Merge Worker 不属于 Execution Plane。
它属于 Verification / Integration Plane。
```

失败行为：

```text
forbidden_files touched → block
file claim conflict → wait / split / human review
checkpoint save failed → pause / retry / human review
worker stuck → supervisor recovery
```

## 4.6 Verification / Integration Plane

负责质量控制、集成和最终准入。

组件：

```text
Verifier
Reviewer
Scoring Engine
Fragment Integrator
Merge Worker
Final Gate
Artifact Gate
Quality Gate
```

职责：

```text
测试验证
artifact 校验
质量评分
patch / fragment 合并
最终准入
项目状态回写
```

失败行为：

```text
Artifact Gate fail → fail_retryable / fail_terminal
Quality Gate requires_human_review → blocked
Final Gate fail → Project Board item failed
Final Gate pass_with_notes → Project Board item review
Final Gate pass → Project Board item done
```

## 4.7 Memory / Optimization Plane

负责沉淀经验。

组件：

```text
Run Logs
Eval Records
Retrospective
Skill Extractor
Skill Library
Baseline Store
Routing Optimizer
Policy Candidate Store
keep_rate_observation
user_feedback_event
failure clusters
```

职责：

```text
保留事实与经验
沉淀技能
记录 baseline
记录反馈与长期价值信号
生成候选策略线索
```

禁止：

```text
不直接改 runtime
不直接改 Project Brief
不直接部署 policy
不把一次失败自动写成永久规则
不让 LLM critique 成为未经 review 的 canonical memory
```

## 4.8 Governance & Policy Plane

v1.3.2 新增核心 Plane。

组件：

```text
policy_candidate_manifest
candidate_evidence_pack
evaluation_admission_contract
approval_record
rollback_plan
deployment_scope
policy_registry
policy_candidate_lifecycle
```

职责：

```text
证据准入
候选策略生命周期
人工审批记录
回滚计划
部署范围控制
策略注册表维护
```

它不是 agent，不执行任务，不拥有目标。

## 4.9 权责边界

```text
Project Brief = 目标来源
Project Board = 状态事实源
Quality Gate = 质量 / 风险评估器
Final Gate = 最高准入函数
Memory / Optimization Plane = 经验沉淀层
Governance & Policy Plane = 策略候选、证据准入、审批、回滚与部署范围控制层
Orchestrator = deterministic coordination / state progression controller
```

## 4.10 冲突优先级

```text
1. Project Brief / Human Owner approval
2. Final Gate
3. Quality Gate
4. Governance & Policy Plane
5. Evaluation Plane
6. Memory / Optimization Plane
7. Orchestrator execution flow
8. Advisor / model / skill suggestion
```

Advisor、model、skill、routing experiment 永远不能成为最高目标来源或最高准入函数。

---

# 5. 所有组件详细规范

## 5.1 Project Brief

职责：记录项目目标、背景、约束、成功标准和人类负责人。

输入：

```text
human intent
project requirements
constraints
non-goals
risk level
```

输出：

```text
project_brief schema
objective
constraints
success criteria
delivery artifacts
```

规则：

```text
Project Brief 是最高目标来源。
任何目标重写必须由 Human Owner 批准。
Final Gate 不能改写 Project Brief。
```

失败行为：

```text
brief 不完整 → blocked / needs_human_clarification
brief 与 policy 冲突 → requires_human_review
```

## 5.2 Project Architect / PM Agent

职责：项目级拆分、模块边界、测试策略、依赖图、回滚策略。

输入：

```text
Project Brief
existing project state
constraints
risk register
```

输出：

```text
project_architecture_plan
module_contracts
test_case_packs
project_dependency_graph
```

规则：

```text
必须使用 strong_planner。
只能写项目管理产物。
不能写 src/、tests/、runtime/、.runtime/、runs/ 或 .git/。
```

失败行为：

```text
dependency graph invalid → handoff blocked
module contract missing → item remains todo
test case pack missing → item cannot enter write phase
```

## 5.3 Project Board

职责：项目状态事实源。

输入：

```text
project_architecture_plan
project_item_state_changed events
final_gate decisions
dependency resolution
```

输出：

```text
project item state
ready items
blocked items
review items
done items
failed items
```

规则：

```text
Task Queue 不是项目事实源。
Task completed 只能让 item 进入 review。
Final Gate 是唯一通向 done 的路径。
```

失败行为：

```text
state transition invalid → trajectory anomaly
missing handoff → projection warning / error
conflicting status → requires_human_review
```

## 5.4 Project Dependency Graph

职责：表达项目级依赖。

输入：

```text
module_contracts
project board items
required artifacts
approval dependencies
```

输出：

```text
dependency unlock decisions
project_dependency_resolved events
blocked / ready state updates
```

规则：

```text
hard_dependency 阻断 write phase。
artifact_dependency 由 Artifact Gate / Lifecycle Manager 解锁。
soft_dependency 允许 readonly prefetch。
approval_dependency 必须等 Approval Broker 决策。
```

失败行为：

```text
cycle detected → block
missing upstream → block
artifact unavailable → wait / block
approval missing → WAITING_APPROVAL
```

## 5.5 Module Contract

职责：定义模块输入、输出、测试、上下文预算、依赖、允许文件和回滚策略。

规则：

```text
每个 module 必须有 input/output/test/context contract。
模块必须小到能在单个 active context 内完成。
```

失败行为：

```text
missing input/output contract → cannot queue
context budget exceeded → split module / ask approval
allowed_files incomplete → scope correction required
```

## 5.6 Test Case Pack

职责：定义 required、optional、regression tests。

规则：

```text
每个 module 至少有一个 required test。
integration module 必须有 integration test。
config/rule module 必须有 negative test 或 safety check。
bugfix module 必须有 reproduction path。
doc module 必须有 review checklist。
```

失败行为：

```text
required tests missing → cannot enter ready
test failure → Quality Gate fail_retryable / fail_terminal
```

## 5.7 Project-to-Queue Handoff

职责：把 ready items 送入 Batch Task Queue。

规则：

```text
只有 status=ready 的 project item 可以进入 Task Queue。
依赖未满足的 item 不能进入 write phase。
可先进入 readonly prefetch。
Project Board 更新必须由 Kernel 记录事件。
Task 完成后必须回写 Project Board。
```

失败行为：

```text
dependency unresolved → WAITING_DEPENDENCY
approval missing → WAITING_APPROVAL
invalid handoff → block
```

## 5.8 Harness Kernel / Orchestrator Core

职责：确定性推进状态，不拥有目标，不拥有长期记忆，不直接改策略。

输入：

```text
events.jsonl
task records
project board state
queue state
gate results
```

输出：

```text
validated event log
project projection
task projection
state changes
digest inputs
```

规则：

```text
append-only event log。
event_id 全局唯一。
idempotency_key 支持幂等 no-op 和 conflict detection。
docs/stage0/events.jsonl 永不修改。
```

失败行为：

```text
invalid JSONL → fail
duplicate event_id → reject
idempotency conflict → reject
missing newline → reject
```

## 5.9 EventStore

职责：append-only event 持久化、JSONL 校验、幂等性检查。

输入：

```text
event dict
event log path
```

输出：

```text
events.jsonl append
validation report
replay preflight report
event id set
```

失败行为：

```text
MissingNewlineError
InvalidJsonLineError
DuplicateEventIdError
DuplicateIdempotencyConflictError
SchemaViolationError
ReplayPreflightError
```

## 5.10 ProjectionStore

职责：从 events replay Project State / Task Queue State。

失败行为：

```text
bad fixture → reject
missing parent → warning / preflight issue
duplicate event → reject
```

## 5.11 Task Queue Manager

职责：维护任务队列状态。

状态：

```text
QUEUED
TRIAGED
READY
READY_READONLY
READY_WRITE
RUNNING
WAITING_APPROVAL
PAUSED_BUDGET
WAITING_DEPENDENCY
BLOCKED
BLOCKED_UPSTREAM_FAILED
BLOCKED_APPROVAL
BLOCKED_PROVIDER
COMPLETED
FAILED
CANCELLED_BY_DEPENDENCY
```

## 5.12 TaskRecordStore

职责：读取 task_spec、completion、handoff_pack、run_log 等 task record。

失败行为：

```text
missing completion → fail
invalid handoff_pack → fail
missing evidence_refs → warning / fail depending gate
```

## 5.13 BatchRunner

职责：列出 ready items，预构建并验证 planned events，再 append。

失败行为：

```text
protected source event log append target → refuse
planned event invalid → reject before append
```

## 5.14 FinalGateRunner

职责：最高执行准入函数。决定 project item 是否可从 review 进入 done。

输入：

```text
completion.json
handoff_pack
run_log
artifact refs
quality gate result
project constraints
```

输出：

```text
pass
pass_with_notes
fail
requires_human_review
```

规则：

```text
task completed ≠ project item done。
Final Gate 是唯一通往 done 的路径。
```

## 5.15 Scoring Engine

职责：rule-based score，不是 LLM judge。

输出：

```text
score
grade
risk notes
quality deltas
```

## 5.16 Artifact Gate

职责：

```text
Artifact Existence Check
Schema Check
Evidence Refs Check
Allowed / Forbidden Files Check
Completion / Handoff Consistency Check
```

失败行为：

```text
artifact missing → fail
schema invalid → fail
forbidden file touched → block
allowed files incomplete → scope correction required
```

## 5.17 Quality Gate Manager

职责：聚合 score、artifact、trajectory、baseline、human-review needs。

结果：

```text
pass
pass_with_notes
fail_retryable
fail_terminal
requires_human_review
```

规则：

```text
requires_human_review never auto-transitions。
```

## 5.18 Trajectory Monitor

职责：

```text
Event-Level Drift Checks
Repeated Failure Checks
Loop Detection
Missing Handoff Detection
Excessive Retry Detection
```

失败行为：

```text
drift anomaly → diagnostic / fail depending severity
loop detected → requires_human_review
missing handoff → fail or warning
```

## 5.19 Evaluation Runner

职责：运行 controlled evaluation targets。

目标：

```text
sanitized Stage 0 fixture
known bad fixture
task record fixtures
orchestrator full flow
edge cases
real-world read-only fixtures
harness change evaluation
```

## 5.20 Advisor Broker

职责：Advisor Protocol 生命周期：preflight、correction、arbitration、risk_scan。

输入：

```text
AdvisorContextPack
Advisor call type
Token budget
```

输出：

```text
AdvisorResponse
```

失败行为：

```text
Budget exceeded returns budget_exceeded error, not a model call.
Missing context fields produce structured error response, not a crash.
Stub provider never fails unexpectedly.
```

## 5.21 Model Gateway

职责：统一模型 provider 调用接口。当前 stub-first。

Model tiers：

```text
strong_planner
cheap_executor
verifier
advisor
```

规则：

```text
No real model calls until explicitly allowed.
Every component that will eventually call a real model MUST start as deterministic stub.
Swapping to a real provider is configuration change, not architecture change.
```

## 5.22 Routing Experiment Manager

职责：比较 routing policy，observational only。

规则：

```text
RoutingExperimentReport 可以 recommend。
不能自动修改 active routing policy。
human approval required。
```

## 5.23 Sampling Runner

职责：运行 N variants，评分并选择最佳候选。

规则：

```text
stub variations deterministic。
real model sampling 需要显式批准。
```

## 5.24 Skill Extractor / Skill Library

职责：从 run_log、retrospective、advisor_response、completion 中抽取技能。

规则：

```text
Skill extraction 不自动改 prompt。
Skill record 必须进入 review / approval 后才可用。
Skills-first 优先于 specialist agents。
```

## 5.25 Dynamic DAG Manager

职责：

```text
DAGNode
DAGEdge
DAGState
DAGMutation
DAGMutationPolicy
DAGManager
```

支持 mutation：

```text
add_node
remove_node
split_node
retry_node
pause_node
resume_node
replace_edge
rollback
```

规则：

```text
每个 mutation 必须可审计。
必须检测 cycle。
必须限制节点数 / 边数。
高风险 mutation 需要 approval。
```

## 5.26 Sandbox Manager

职责：

```text
SandboxHandle
WriteClaim
ConflictDetection
create / claim / release / conflict / export / cleanup
```

规则：

```text
主仓库默认只读。
所有写入进入 sandbox / worktree。
真实 filesystem operations 需要单独批准。
tests use temp directories only。
```

## 5.27 Concurrency Controller

职责：受控并发调度。

规则：

```text
Sequential baseline remains default.
max_concurrent limit required.
Items sharing any file in allowed_files cannot run in parallel.
Hard dependencies must be done before downstream enters running.
Artifact dependencies require verified artifact.
Soft dependencies advisory only.
```

## 5.28 Runtime Supervisor

职责：

```text
event log integrity check
projection consistency check
worker liveness
checkpoint store check
stuck detection
recovery plan creation
```

失败行为：

```text
worker stuck → retry from checkpoint or escalate
checkpoint missing → restart_in_new_sandbox / human review
```

## 5.29 Checkpoint Manager

职责：

```text
save checkpoint
load checkpoint
list checkpoints
create recovery plan
```

规则：

```text
Checkpoint save with same ID overwrites idempotently.
Recovery skips completed steps.
Event append remains idempotent.
```

## 5.30 Artifact Lifecycle Manager

状态：

```text
draft
produced
verified
rejected
promoted
archived
```

规则：

```text
When artifact reaches verified or promoted, check_unlocks() is called.
Dependency unlocking recorded as project_dependency_resolved event.
```

## 5.31 Health Monitor

职责：聚合 component health。

输出：

```text
ComponentHealth
SupervisorReport
DashboardSnapshot input
```

## 5.32 Dashboard Data Model

职责：仅数据模型，没有 Web UI。

视图：

```text
Batch Digest View
Approval Queue
Task State Board
Event Stream
DAG Visualization
Quality Scores
Skill Library
```

---

# 6. 所有协议 Schema

## 6.1 project_brief

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

## 6.2 project_architect_node

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

## 6.3 project_architecture_plan

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

## 6.4 project_board

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

## 6.5 project_dependency_graph

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

## 6.6 module_contract

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

## 6.7 module_context_contract

```yaml
module_context_contract:
  max_active_context_tokens: 200000
  preferred_context_tokens: 80000
  input_contract_required: true
  output_contract_required: true
  test_case_required: true
  artifact_refs_only: true
```

## 6.8 test_case_pack

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

## 6.9 project_to_queue_handoff

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

## 6.10 project_board_task_status_mapping

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

## 6.11 final_gate_to_project_board_mapping

```yaml
final_gate_to_project_board_mapping:
  pass:
    project_status: done
  pass_with_notes:
    project_status: review
  fail:
    project_status: failed
```

## 6.12 node_contract

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

## 6.13 events.jsonl

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

## 6.14 project_event_payload

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

## 6.15 completion.json

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

## 6.15a write_claim

`write_claim` 是 Sandbox Manager 与 Concurrency Controller 的核心 schema。它决定一个节点是否可以获得写入权限，以及多个候选执行是否可以并行。

```yaml
write_claim:
  task_id:
  node_id:
  sandbox_id:
  files_exact:
    - src/a.py
    - tests/test_a.py
  file_patterns:
    - src/module_a/**
  uncertainty: exact | estimated | unknown
  lock_mode:
    - exclusive
    - candidate_isolated
    - readonly
```

规则：

```text
exact 文件集合不重叠 → 可并行 Builder。
文件集合重叠但在不同 candidate sandbox → 可并行，但最终 merge 串行。
文件集合 unknown → 先运行 Code Scanner 缩小范围；仍 unknown 则保守排队。
修改全局配置、lockfile、schema、registry → 需要 repo-level write lock。
completion.json 中的 write_claims_released 只表示该 claim 已释放，不替代 write_claim schema。
```

## 6.16 checkpoint

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

## 6.17 handoff_pack

```yaml
handoff_pack:
  structured_fields:
  summary:
  evidence_refs:
  full_artifact_refs:
```

## 6.18 artifact

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

## 6.19 approval_request

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

## 6.20 batch_digest

`batch_digest` 是无人值守 batch run 的主界面，不只是摘要格式。它必须同时承载推荐动作和人工响应回流入口。

```yaml
batch_digest:
  batch_id:
  overnight_summary:
  completed_tasks:
    - task_id:
      title:
      final_status:
      important_artifacts:
      acceptable:
  blocked_or_waiting_approval:
    - task_id:
      title:
      blocked_reason:
      approval_request_ref:
      recommended_action:
  failed_tasks:
    - task_id:
      title:
      failure_code:
      failure_summary:
      fallback_attempts:
      recommended_next_action:
  risk_cost_report:
    total_tokens:
    harness_overhead_ratio:
    advisor_calls:
    model_cost_estimate:
    runtime:
  recommended_actions:
    - action_id:
      task_id:
      recommended_action:
      alternatives:
      reason:
      risk_level:
      expected_next_step:
```

### 6.20.1 digest_action

```yaml
digest_action:
  action_id: act_20260508_003
  task_id: task_api_design_001
  recommended_action: approve_merge
  alternatives:
    - reject
    - rerun
    - approve_readonly_only
    - defer
  reason:
  risk_level:
  expected_next_step:
```

### 6.20.2 human_decision_event

```yaml
human_decision_event:
  action_id:
  decision:
  constraints:
  timestamp:
```

规则：

```text
Digest Action 由 Approval Broker、Final Gate、Policy Engine、Reviewer / Advisor 共同提供理由。
Human Decision Event 必须写回事件流，不能只停留在 UI 或聊天记录中。
recommended_actions 里的每一项都必须可追溯到 digest_action。
人工 approve / reject / defer / approve_readonly_only 等响应必须转成 human_decision_event，以便 Kernel 后续 replay。
```

## 6.21 advisor_context_pack_v2

```yaml
advisor_context_pack_v2:
  schema_version: advisor_context_pack.v2
  pack_id:
  task_id:
  item_id:
  call_type: preflight | correction | arbitration | risk_scan
  objective:
  current_status:
  failure_code:
  failure_subcode:
  risk_level:
  acceptance_criteria:
  allowed_files:
  forbidden_files:
  artifact_refs:
    - artifact_id:
      artifact_type:
      path:
      status:
      content_hash:
  evidence_refs:
    - ref_id:
      ref_type: run_log | completion | handoff_pack | artifact | event | digest
      path:
      summary:
  quality_signals:
    score:
    grade:
    gate_result:
    anomalies:
    tool_errors:
  budget:
    max_context_tokens:
    preferred_context_tokens:
    max_response_tokens:
  retrieval_policy:
    allow_retrieval: true
    allowed_ref_types:
    forbidden_paths:
  omitted_context_notice:
  created_at:
```

## 6.22 model_context_pack_v2

```yaml
model_context_pack_v2:
  schema_version: model_context_pack.v2
  pack_id:
  task_id:
  item_id:
  model_tier:
  model_harness_profile_id:
  role: planner | executor | debugger | verifier | advisor | integrator
  task_summary:
  input_contract:
  output_contract:
  acceptance_tests:
  current_project_status:
  dependency_summary:
  allowed_tools:
  forbidden_tools:
  allowed_files:
  forbidden_files:
  artifact_refs:
  evidence_refs:
  quality_warnings:
  context_budget:
    max_context_tokens:
    preferred_context_tokens:
    reserved_response_tokens:
  retrieval_policy:
    allow_retrieval:
    max_retrieval_calls:
    allowed_ref_types:
  switch_context:
    previous_model_tier:
    previous_tool_formats:
    forbidden_previous_tools:
    handoff_summary:
  created_at:
```

## 6.23 context_retrieval_request

```yaml
context_retrieval_request:
  request_id:
  requester_id:
  requester_type: advisor | model | verifier | human | evaluator
  task_id:
  reason:
  requested_refs:
    - ref_id:
      ref_type: run_log | completion | handoff_pack | artifact | event | digest | source_excerpt
      path:
      requested_scope: summary | excerpt | full
  token_budget:
  priority: low | normal | high
  created_at:
```

## 6.24 context_retrieval_result

```yaml
context_retrieval_result:
  request_id:
  result_id:
  status: fulfilled | partial | denied | not_found | budget_exceeded
  returned_refs:
    - ref_id:
      ref_type:
      path:
      content_mode: summary | excerpt | full
      content:
      content_hash:
      token_estimate:
  denied_refs:
    - ref_id:
      reason:
  total_token_estimate:
  budget_remaining:
  created_at:
```

## 6.25 context_layers extension

```yaml
context_layers:
  invariants:
  task_pack:
  dynamic_refs:
  memory_digest:
  recent_evidence:
```

说明：

```text
v1.2 四 schema = 对外协议 / wire format canonical source。
v1.3.2 五层结构 = 对内编排布局 / pack compiler target。
不得另行创建第五套 context_pack schema。
如需新增字段，应作为 v1.2 advisor_context_pack_v2 / model_context_pack_v2 的 extension fields 或 nested context_layers，而不是替换原 schema。
```

## 6.26 keep_rate_observation

```yaml
keep_rate_observation:
  observation_id:
  task_id:
  artifact_id:
  repo_ref:
  observation_window: 6h | 24h | 7d | 30d
  produced_patch_hash:
  retained_patch_hash:
  produced_lines:
  retained_lines:
  retained_ratio:
  status: retained | partially_rewritten | reverted | superseded | unknown
  evidence_refs:
  observed_at:
```

## 6.27 user_feedback_event

```yaml
user_feedback_event:
  feedback_id:
  task_id:
  artifact_id:
  user_action: accepted | modified | rejected | ignored | requested_changes
  user_comment:
  satisfaction_label: satisfied | partially_satisfied | dissatisfied | unclear
  label_source: human | future_llm_judge
  linked_event_ids:
  created_at:
```

## 6.28 harness_maintenance_issue

```yaml
harness_maintenance_issue:
  issue_id:
  source: weekly_log_review | eval_regression | unknown_error | user_feedback | keep_rate_drop
  title:
  severity: low | medium | high | critical
  affected_components:
  evidence_refs:
  proposed_action:
  status: proposed | accepted | rejected | resolved
  created_at:
```

## 6.29 model_harness_profile

```yaml
model_harness_profile:
  profile_id:
  tier:
  provider:
  model_id:
  tool_format:
  prompt_style:
  supports_patch_edit:
  supports_string_replace:
  supports_tool_use:
  supports_caching:
  context_window:
  cost_metadata:
    input_cost_per_1k:
    output_cost_per_1k:
    cache_read_cost_per_1k:
    cache_write_cost_per_1k:
  handoff_summary_required:
  switch_instructions_required:
  forbidden_previous_tools:
    - tool_id:
      tool_type:
      reason:
      replacement_tool_id:
      enforcement_scope: prompt_assembly | gateway_validation | context_broker | all
  allowed_tools:
    - tool_id:
      tool_type:
  context_pack_schema: advisor_context_pack.v2 | model_context_pack.v2
  max_tool_calls_per_turn:
  max_retrieval_calls_per_turn:
```

## 6.30 model_harness_profile v1.3.2 minimum fields

```yaml
model_harness_profile:
  profile_id:
  provider:
  model_id:
  tier:
  tool_strictness:
  json_tolerance:
  reasoning_effort:
  output_format_expectation:
  parallel_tool_preference:
  escaping_quirks:
  cache_strategy:
  fallback_policy:
  context_window:
  cost_metadata:
  allowed_tools:
  forbidden_previous_tools:
```

说明：v1.2 的 `model_harness_profile` 完整字段保留；v1.3.2 的 minimum fields 是后续 shadow routing track 的最低字段集，不替代 v1.2 schema。

## 6.31 policy_candidate_manifest

```yaml
policy_candidate_manifest:
  schema_version: policy_candidate.v1
  candidate_id:
  candidate_type: context_pack | tool_contract | routing_rule | skill_package | eval_gate | error_taxonomy | model_profile
  title:
  rationale:
  source_refs:
  proposed_change_summary:
  affected_components:
  expected_benefit:
  expected_risk:
  required_evidence:
  evaluation_plan:
  rollback_plan_ref:
  approval_required: true
  created_at:
```

## 6.32 candidate_evidence_pack

```yaml
candidate_evidence_pack:
  schema_version: candidate_evidence.v1
  candidate_id:
  admitted_evidence_refs:
  diagnostic_evidence_refs:
  fixture_results:
  quality_deltas:
  cost_deltas:
  failure_cluster_refs:
  human_review_refs:
  recommendation: accept | reject | revise | needs_more_evidence
```

## 6.33 approval_record

```yaml
approval_record:
  schema_version: approval_record.v1
  candidate_id:
  approver:
  decision: approved | rejected | deferred
  rationale:
  required_followups:
  deployment_scope:
  rollback_required: true
  approved_at:
```

## 6.34 policy_registry_entry

```yaml
policy_registry_entry:
  schema_version: policy_registry.v1
  policy_id:
  candidate_id:
  policy_type:
  status: proposed | approved | active | rolled_back | retired
  active_scope:
  version:
  evidence_pack_ref:
  approval_ref:
  rollback_plan_ref:
  activated_at:
  retired_at:
```

## 6.35 rollback_plan

```yaml
rollback_plan:
  schema_version: rollback_plan.v1
  rollback_plan_id:
  candidate_id:
  policy_id:
  rollback_scope: docs_only | config | schema | profile | skill | eval_gate | runtime_guard
  trigger_conditions:
    - regression_detected
    - cost_threshold_exceeded
    - quality_gate_failure
    - human_rejection
    - unknown_error_increase
  impacted_refs:
    - path_or_registry_key:
      ref_type: file | registry_entry | config_key | schema_id | skill_id | model_profile_id
      pre_change_ref:
      post_change_ref:
  rollback_steps:
    - step_id:
      action: revert_file | restore_registry_entry | disable_policy | restore_schema | retire_skill | restore_profile
      target_ref:
      expected_result:
  validation_steps:
    - run_fixed_eval_suite
    - run_harness_change_eval
    - run_relevant_unit_tests
    - verify_policy_registry_status
  rollback_owner:
  max_rollback_time:
  fallback_policy:
  status: proposed | approved | executed | failed | obsolete
  created_at:
```

## 6.36 usage_ledger

```yaml
usage_ledger:
  schema_version: usage_ledger.v1
  run_id:
  case_id:
  input_tokens:
  output_tokens:
  cached_tokens:
  request_count:
  tool_call_count:
  retry_count:
  wall_clock_ms:
  estimated_cost:
  pass:
  cost_of_pass_group:
  model_profile_id:
  context_pack_id:
```

## 6.37 error_record

```yaml
error_record:
  schema_version: error_record.v1
  error_id:
  error_domain:
  error_class:
  retryable:
  counts_against_model:
  requires_human_triage:
  tool_name:
  model_profile_id:
  context_pack_id:
  event_id:
  evidence_refs:
  created_at:
```

## 6.38 fixture_metadata

```yaml
fixture_metadata:
  fixture_id:
  source_type: synthetic | copied_real_read_only | mutated_user_style
  freshness:
  estimated_human_minutes:
  difficulty:
  contamination_risk: low | medium | high | unknown
  admission_scope: admitted | diagnostic
```

## 6.39 AdvisorContextPack dataclass

```python
@dataclass(frozen=True)
class AdvisorContextPack:
    task_id: str
    call_type: str  # preflight | correction | arbitration | risk_scan
    task_spec: dict[str, Any]
    completion: dict[str, Any] | None
    handoff_pack: dict[str, Any] | None
    run_log_text: str | None
    failure_code: str | None
    project_context: dict[str, Any] | None
```

## 6.40 AdvisorResponse dataclass

```python
@dataclass(frozen=True)
class AdvisorResponse:
    call_type: str
    diagnosis: str
    recommended_action: str
    do_not_do: str
    confidence: float  # 0.0 - 1.0
    token_usage: int
    provider: str  # "stub" | provider name
    raw_response: dict[str, Any] | None = None
```

## 6.41 AdvisorBudget dataclass

```python
@dataclass(frozen=True)
class AdvisorBudget:
    max_tokens: int
    max_calls_per_task: int
    current_calls: int = 0
    current_tokens: int = 0
```

---

# 7. Policy Engine 规则集

## 7.1 定位

Stage 1 的 Policy Engine 是 Kernel 内部 deterministic rule evaluator。模型可以解释，但不能决定。

输入：

```text
task state
risk level
verifier result
reviewer verdict
artifact gate result
approval status
budget state
failure code
sandbox/write claim state
```

输出：

```text
recommended action
allowed actions
blocked actions
reason
rule_id
policy_version
```

## 7.2 规则字段语义

```text
layer：规则所属全局层级。跨层冲突时，layer precedence 永远优先于 priority。
priority：只在同一 layer 内排序，不跨 layer 比较。Safety priority 100 与 Budget priority 100 不竞争。
severity：同一 layer 内多个规则同时匹配时的保守程度，用于冲突处理。
```

Layer 顺序：

```text
Safety > Approval > Budget > Dependency > Quality > Optimization
```

## 7.3 Canonical when 条件格式

Policy Engine 只支持字段值匹配与显式操作符对象，不允许把操作符嵌入 key 名。

```yaml
when:
  field: "exact_value"
  field_list:
    in: ["a", "b"]
  numeric_field:
    gte: 0.85
  other_numeric_field:
    lte: 100
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

非法格式：

```yaml
when:
  budget_usage_gte: 0.85
```

## 7.4 severity 枚举分层

本文保留两套 severity，它们用途不同，不能混用。

### 7.4.1 Policy severity

用于 Policy Engine 的动作阻断和冲突解决：

```text
info
warning
block
critical
```

语义：

```text
info：记录，不影响执行。
warning：进入 digest / quality notes，但不阻断。
block：阻断当前动作，允许修正后继续。
critical：阻断并要求 human review。
```

### 7.4.2 Risk severity

用于风险、反馈、维护 issue、model profile、harness maintenance 等评估场景：

```text
low
medium
high
critical
```

语义：

```text
low：低风险，可自动记录或进入普通 digest。
medium：中风险，可继续只读或请求较轻审批。
high：高风险，默认需要人工审批。
critical：关键风险，必须阻断并进入人工处理。
```

规则：

```text
Policy Engine 的 then.severity 可以使用 low/medium/high/critical 或 info/warning/block/critical，但实现时必须归一化到动作等级。
low ≈ info，medium ≈ warning，high ≈ block，critical = critical。
风险报告和维护 issue 不得使用 block 作为风险等级。
Policy 冲突解决不得把 medium/high 当成普通字符串排序，必须经过 severity normalization。
```

## 7.5 冲突解决算法

```text
1. 收集所有匹配规则。
2. 按 layer 顺序排序：Safety > Approval > Budget > Dependency > Quality > Optimization。
3. 先比较 layer，不跨 layer 比较 priority。
4. 同一 layer 内，按 priority 从高到低排序。
5. 同一 layer、同一 priority 内，按 normalized severity 取最严格结果。
6. block / critical 优先于 warning / info。
7. Safety layer 的 block 不允许被 Optimization layer 覆盖。
8. Approval layer 的 human decision 不允许被 Advisor 或 routing recommendation 覆盖。
9. 所有匹配规则的 blocked_actions 取并集。
10. 所有匹配规则的 allowed_actions 取交集。
11. 如果 allowed_actions 交集为空，则使用最高优先级 layer 中最高 priority 规则的 allowed_actions，并保留全部 blocked_actions 并集。
12. 如果某 action 同时出现在 allowed_actions 和 blocked_actions，blocked_actions 优先。
13. auto_execute 只有在所有匹配规则都允许 auto_execute 且无 block/critical 时才为 true。
14. 输出必须包含 matched_rule_ids、effective_layer、effective_priority、effective_severity、allowed_actions、blocked_actions、recommendation。
```

## 7.6 policy.yaml Stage 1 Starter Set

以下不是示例，而是 Stage 1 的最小 starter set。实现时可以扩展，但不能省略这些规则。每条规则的 `then.severity` 是冲突解决算法的必需字段。

```yaml
policy_version: "policy.v1"

rules:
  - rule_id: "safety.no_destructive_auto_approve"
    layer: Safety
    priority: 100
    when:
      requested_action:
        in: ["delete_files", "submit_pr", "external_side_effect"]
    then:
      recommendation: "wait_for_human_approval"
      severity: critical
      allowed_actions:
        - approve
        - reject
        - defer
      blocked_actions:
        - auto_execute
      auto_execute: false

  - rule_id: "safety.scope_violation_block_human"
    layer: Safety
    priority: 95
    when:
      failure_code: "F007_SCOPE_VIOLATION"
    then:
      recommendation: "block_and_ask_human"
      severity: critical
      allowed_actions:
        - approve_with_constraints
        - reject
        - rerun_in_clean_sandbox
      blocked_actions:
        - merge
        - auto_retry_without_review
      auto_execute: false

  - rule_id: "approval.destructive_timeout_wait_forever"
    layer: Approval
    priority: 92
    when:
      requested_action:
        in: ["delete_files", "submit_pr", "access_external_service"]
      approval_status: "timeout"
    then:
      recommendation: "wait_forever"
      severity: critical
      allowed_actions:
        - approve
        - reject
        - defer
      blocked_actions:
        - auto_execute
      auto_execute: false

  - rule_id: "approval.high_risk_timeout_wait_forever"
    layer: Approval
    priority: 90
    when:
      risk_level: "high"
      approval_status: "timeout"
    then:
      recommendation: "wait_forever"
      severity: high
      allowed_actions:
        - approve
        - reject
        - defer
      blocked_actions:
        - auto_execute
      auto_execute: false

  - rule_id: "approval.medium_risk_timeout_downgrade_readonly"
    layer: Approval
    priority: 80
    when:
      risk_level: "medium"
      approval_status: "timeout"
    then:
      recommendation: "downgrade_to_readonly"
      severity: medium
      allowed_actions:
        - approve_readonly_only
        - defer
        - reject
      blocked_actions:
        - write
        - merge
      auto_execute: false

  - rule_id: "budget.hard_limit_force_single_path"
    layer: Budget
    priority: 85
    when:
      budget_usage:
        gte: 0.85
    then:
      recommendation: "force_single_path"
      severity: high
      allowed_actions:
        - continue_single_path
        - stop
        - ask_human
      blocked_actions:
        - build_sampling
        - nonessential_advisor
        - extra_review_sampling
      auto_execute: true

  - rule_id: "budget.kill_limit_pause_for_approval"
    layer: Budget
    priority: 100
    when:
      budget_usage:
        gte: 1.0
    then:
      recommendation: "pause_and_request_approval"
      severity: critical
      allowed_actions:
        - approve_more_budget
        - stop
        - defer
      blocked_actions:
        - continue_spending
      auto_execute: false

  - rule_id: "dependency.provider_unavailable_block"
    layer: Dependency
    priority: 75
    when:
      task_state: "BLOCKED_PROVIDER"
    then:
      recommendation: "include_in_batch_digest"
      severity: high
      allowed_actions:
        - retry_later
        - switch_approved_provider
        - defer
      blocked_actions:
        - silent_unknown_model_fallback
      auto_execute: false

  - rule_id: "dependency.artifact_gate_readonly_only"
    layer: Dependency
    priority: 70
    when:
      artifact_gate_decision: "unlock_readonly"
    then:
      recommendation: "continue_readonly_only"
      severity: medium
      allowed_actions:
        - readonly_analysis
        - wait
      blocked_actions:
        - write
        - merge
      auto_execute: true

  - rule_id: "quality.tests_failed_block_merge"
    layer: Quality
    priority: 90
    when:
      verifier_status: "fail"
    then:
      recommendation: "rerun_fix_loop"
      severity: high
      allowed_actions:
        - rerun
        - escalate
        - reject
      blocked_actions:
        - merge
      auto_execute: false

  - rule_id: "quality.context_overflow_split_task"
    layer: Quality
    priority: 80
    when:
      failure_code: "F003_TASK_TOO_LARGE"
    then:
      recommendation: "task_fallback_split"
      severity: medium
      allowed_actions:
        - split_task
        - ask_advisor
        - reject
      blocked_actions:
        - retry_same_large_task
      auto_execute: true

  - rule_id: "quality.format_error_retry_or_model_fallback"
    layer: Quality
    priority: 65
    when:
      failure_code: "F008_FORMAT_ERROR"
    then:
      recommendation: "retry_format_repair_then_model_fallback"
      severity: medium
      allowed_actions:
        - retry_format_repair
        - retry_with_stronger_model
        - reject
      blocked_actions: []
      auto_execute: true

  - rule_id: "optimization.low_risk_readonly_continue"
    layer: Optimization
    priority: 50
    when:
      risk_level: "low"
      requested_action: "readonly_analysis"
    then:
      recommendation: "continue"
      severity: low
      allowed_actions:
        - continue
      blocked_actions: []
      auto_execute: true
```

---

# 8. Evaluation Admission Contract

## 8.1 Admitted Evidence

可以参与 policy adoption 判断的证据：

```text
deterministic fixture result
real-world read-only fixture result
harness change evaluation snapshot diff
quality gate result
final gate result
baseline comparison
manually reviewed user feedback
validated usage ledger / cost-of-pass record
reviewed failure cluster
```

要求：

```text
可复现
可追溯
有 schema
有 evidence_refs
能被 CI 或人工复核
```

## 8.2 Diagnostic Evidence

只能作为诊断参考，不能单独驱动 policy adoption：

```text
LLM critique
Advisor suggestion
one-off benchmark result
exploratory trace
non-reviewed user comment
single-run model score
unreviewed skill extraction
shadow router recommendation
```

规则：

```text
diagnostic evidence 可以生成 policy candidate。
diagnostic evidence 不能直接部署 policy。
diagnostic evidence 必须通过 admitted evidence 转化后才能进入 approval decision。
```

---

# 9. Policy Candidate Lifecycle

受控自适应的最小闭环：

```text
Monitor
  -> collect traces / snapshots / tool errors / usage / feedback
Analyze
  -> cluster failure / detect regression / identify opportunity
Candidate
  -> generate limited policy/config/schema/profile/skill candidate
Evaluate
  -> run fixed fixtures + realistic read-only + user-style mutation + cost checks
Review
  -> Quality Gate + Final Gate + human approval
Deploy
  -> apply harness config only, with rollback plan
Record
  -> write decision and evidence to policy registry
```

规则：

```text
policy_candidate_manifest 必须先于 evaluation。
candidate_evidence_pack 必须区分 admitted_evidence_refs 与 diagnostic_evidence_refs。
approval_record 必须记录 approver、decision、rationale 和 deployment_scope。
rollback_plan 必须在 policy adoption 前存在。
policy_registry_entry 是 policy 状态事实源。
```

---

# 10. CA Maturity Gates

v1.3.2 的 CA gates 是最高成熟度判定标准。

```text
CA-0: Orchestrator Kernel sealed
  Stage 0–4 complete, CI passing, runtime boundary documented.

CA-1: Evaluation suite stable
  deterministic fixtures + realistic read-only fixtures + harness change evaluation all passing.

CA-2: Tool/Error Taxonomy operational
  error domains defined, unknown_error requires triage, error fixtures pass.

CA-3: User-Style Mutation Eval stable
  formal_issue / user_style_chat_request / terse_ticket variants exist for representative fixtures and do not break admission logic.

CA-4: Context Pack v2 schema and tests ready
  v1.2 canonical schemas remain intact; v1.3.2 context_layers mapping, prune policy, retrieval policy, and memory boundary are tested offline.

CA-5: Usage Ledger and Cost-of-Pass available
  eval rows carry token/cost/retry/tool-call data and valid cost_of_pass_group.

CA-6: Policy Candidate Lifecycle implemented
  candidate manifest, evidence pack, approval record, rollback plan, policy registry exist.

CA-7: Governance approval path enforced
  no policy adoption without admitted evidence, rollback plan, and human approval.

CA-8: Advisor-only real model allowed
  model may critique/advice only; no file mutation, shell execution, sandbox execution, or PR.
```

Classification rule：

```text
CA-0 到 CA-2：adaptive-ready
CA-3 到 CA-5：evaluation-controlled adaptive preparation
CA-6 到 CA-7：Controlled Adaptive Orchestrator Kernel
CA-8：controlled real-model advisory mode
```

未达到 CA-6 / CA-7 前，不应声称系统已是 Controlled Adaptive Orchestrator Kernel。

---

# 11. Optional Tracks

Track 顺序以 v1.3.2 Section 15 为准。

## 11.1 Track 1：Tool/Error Taxonomy Hardening

入场条件：

```text
Stage 0–4 complete
CI passing
Harness Change Evaluation available
```

目标：

```text
定义 docs/tool_error_taxonomy.md
新增 tests/test_error_taxonomy.py
新增 tests/fixtures/tool_error_cases/
让 unknown_error、context_error、tool_contract_error、environment_error、model_judgment_error 可被区分
```

退出产物：

```text
error_record schema adopted
error domains defined
unknown_error requires human triage
error fixtures pass
quality digest can report error domain
```

禁止：

```text
不接真实模型
不执行任务
不自动 retry unknown_error
```

## 11.2 Track 2：Realistic User-Style Mutation Eval

入场条件：

```text
Tool/Error Taxonomy 完成或至少有明确 error domains
realistic read-only fixture suite 已存在
```

目标：

```text
为代表性 fixture 创建 formal_issue / user_style_chat_request / terse_ticket 三类表达
加入 fixture_metadata
验证 admission logic 不被输入形式破坏
```

退出产物：

```text
mutated user-style fixtures
fixture_metadata schema validated
CA-3 satisfied
```

## 11.3 Track 3：Context Pack v2 and Memory Boundary

入场条件：

```text
User-Style Mutation Eval 已有代表性 fixture
v1.2 四个 Context Pack schemas 已确认
```

目标：

```text
明确 context_layers
定义 pack_prune_policy
定义 freshness / drop_reason / conflict_resolution
定义 memory boundary
```

退出产物：

```text
Context Pack v2 offline tests
advisor_context_pack_v2 / model_context_pack_v2 extension fields
context_retrieval_request / result tests
CA-4 satisfied
```

## 11.4 Track 4：Usage Ledger / Cost-of-Pass Track

入场条件：

```text
Evaluation suite stable
Context Pack v2 可产生 context_pack_id
```

目标：

```text
记录 token / request / tool_call / retry / wall-clock / estimated cost
定义 cost_of_pass_group
计算 cost-of-pass
```

退出产物：

```text
usage_ledger schema adopted
cost_of_pass_group validated
cost-of-pass report
CA-5 satisfied
```

## 11.5 Track 5：Model Profiles and Shadow Routing

入场条件：

```text
Usage Ledger 已能记录成本
Model Harness Profile schema 已确认
```

目标：

```text
建立 model profile registry
定义 shadow routing
比较 routing suggestion
不自动采用 active routing
```

退出产物：

```text
model_harness_profile docs/tests
shadow routing report
no active routing change without approval
```

## 11.6 Track 6：Skills Registry and Skills-First Policy

入场条件：

```text
Evaluation suite stable
Skill extraction records available
```

目标：

```text
先沉淀程序性技能
避免 premature specialist agents
定义 skill review / approval / retirement
```

退出产物：

```text
Skill Registry
Skill policy tests
no automatic prompt mutation
```

## 11.7 Track 7：Policy Candidate Lifecycle

入场条件：

```text
Tool/Error Taxonomy
User-Style Mutation Eval
Context Pack v2
Usage Ledger
Model Profiles
```

目标：

```text
实现 policy_candidate_manifest
candidate_evidence_pack
approval_record
policy_registry_entry
rollback_plan
```

退出产物：

```text
Policy Candidate Lifecycle documented/tested
rollback_plan required before adoption
CA-6 satisfied
```

## 11.8 Track 8：Advisor-Only Offline Critique

入场条件：

```text
Evaluation Admission Contract 已定义
Policy Candidate Lifecycle 至少文档化
```

目标：

```text
Advisor 作为 offline evaluator / critic / candidate analyst
Advisor 输出属于 diagnostic evidence
```

退出产物：

```text
Advisor critique records
No-advisor baseline comparison
No runtime path dependency
```

## 11.9 Track 9：Advisor-Only Real Model Test

入场条件：

```text
必须满足 Section 12 全部前置条件。
```

目标：

```text
真实模型只做 Advisor Preflight / Correction / Risk Scan / Offline Critique / Candidate Ranking。
```

退出产物：

```text
advisor_response records
usage ledger entries
quality / cost comparison
human review report
```

禁止：

```text
文件修改
shell command
sandbox execution
PR 创建
自动 merge
自动 policy adoption
自动 prompt mutation
```

---

# 12. 真实模型接入前置条件

Advisor-only real model test 前必须满足：

```text
Tool/Error Taxonomy 已完成并通过测试。
User-Style Mutation Eval 已完成代表性 fixture 变体并通过 admission 检查。
Context Pack v2 已定义并通过 offline tests，且与 v1.2 四个 canonical schema 关系明确。
Usage Ledger 已能记录 token/cost/retry/tool-call，并定义有效 cost_of_pass_group。
Model Harness Profile 已定义。
Evaluation Admission Contract 已定义。
Policy Candidate Lifecycle 至少文档化，并包含 rollback_plan schema。
Provider credentials 不进入 repo。
预算上限明确。
人工 approval 明确。
```

真实模型第一次只能做：

```text
Advisor Preflight
Advisor Correction
Advisor Risk Scan
Offline Critique
Candidate Ranking
```

真实模型第一次禁止：

```text
文件修改
shell command
sandbox execution
PR 创建
自动 merge
自动 policy adoption
自动 prompt mutation
```

---

# 13. MVP 分层

Stage 0–4 分层和退出标准来自 v0.7.4.1，不被覆盖。

## 13.1 Stage 0：Manual Project Simulation

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

## 13.2 Stage 1：MVP Batch Runner

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

## 13.3 Stage 2：Quality Runtime

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

## 13.4 Stage 3：Optimization Runtime

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

## 13.5 Stage 4：Autonomous Harness Platform

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

## 13.6 Stage 0 Runbook

目标：

```text
Project Board 是否能承载项目状态
Project Dependency Graph 是否能表达依赖
Module Contract 是否能约束任务边界
Test Case Pack 是否能驱动验收
Project-to-Queue Handoff 是否清楚
Task Runtime schema 是否够用
Batch Digest 是否能作为早晨主界面
```

推荐目录：

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

五个任务模板：

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

# 14. 合并后的设计决定记录

## 14.1 v0.7.x 设计决定

### v0.7.4 正式 supersede v0.7.3 的部分章节

决定：v0.7.4-canonical 正式覆盖 v0.7.3 的 Stage 0、Stage 1、系统 flow、设计决定记录相关部分。  
理由：Project Management Plane 改变了系统入口和 Stage 0 方式，不能再作为 preview 附录存在。

### 新增 Project Management Plane

决定：在 Batch Task Queue 之上新增 Project Management Plane。  
理由：v0.7.3 解决任务执行，但项目开发需要需求、模块、接口、测试、依赖和项目状态管理。

### Project Board 是状态源，Task Queue 是执行队列

决定：Project Board 记录项目事实；Task Queue 只执行 ready items。  
理由：如果直接把项目拆成散任务入队，系统会丢失项目级上下文和依赖关系。

### Project Dependency Graph 必须 schema 化

决定：Project Dependency Graph 有独立 schema，包含 nodes、edges、dependency_type 和 downstream_policy。  
理由：Cross-task Dependency Manager 需要稳定输入，不能从自然语言依赖描述中猜。

### Project Board 与 Task Queue 必须有状态映射表

决定：Task 状态变化必须明确回写 Project Board。  
理由：Project Board 是状态源，如果映射不明确，Stage 1 Kernel 会产生不一致状态。

### Project Architect 显式使用 strong_planner

决定：Project Architect / PM Agent 的 `model_tier` 是 `strong_planner`。  
理由：项目级抽象决定项目质量，不应交给 cheap executor。

### Project Architect 与 Planner / Architect 分层

决定：Project Architect 做项目级模块拆分；Planner / Architect 做任务级 DAG 拆分。  
理由：模块边界和项目质量由上层设计决定，不应交给 Builder 或任务级 Planner 临时决定。

### Merge Worker 只属于 Verification / Integration Plane

决定：Merge Worker 从 Execution Plane Worker 类型中移除，只保留在 Verification / Integration Plane。  
理由：Merge Worker 负责 fragment / patch 冲突解决，属于集成层，不是普通执行层。

### 模块必须有 input/output/test/context contract

决定：每个 module_contract 必须包含输入接口、输出接口、验收标准、测试包和 context budget。  
理由：只有模块边界明确，小 agent 才能安全并行执行。

### 200k 是默认 context budget，不是硬编码规则

决定：`max_active_context_tokens: 200000` 作为默认建议值。  
理由：不同模型上下文不同。真正原则是模块必须小到能在单个 active context 内完成。

### Stage 0 必须从 Project Board 派生任务

决定：Stage 0 的 5 个任务不再是孤立任务，而是从同一个 project_board 派生。  
理由：这样才能验证项目级 workflow，而不是只验证 task runtime。

### Stage 0 五个任务都必须验证 Project Board 状态回写

决定：5 个任务模板全部加入 Project Board 状态回写验收。  
理由：Project Board 是 v0.7.4 核心新增层，必须在 Stage 0 全面验证。

### Project Architect 写权限只限项目管理产物

决定：Project Architect 可以写 `project_board`、`module_contracts/`、`test_case_packs/`、`project_dependency_graph.*`，但不能写 `src/`、`tests/`、`.runtime/`、`runs/` 或 `.git/`。  
理由：Project Architect 负责项目级拆分和管理产物生成，不应绕过 Task Queue、Sandbox Manager 和 Builder 直接修改代码或运行时状态。

### 项目级事件类型进入 Stage 1 Preflight

决定：Stage 1 前必须补齐项目级事件类型，包括 `project_board_item_updated`、`project_to_queue_handoff_created`、`project_dependency_resolved`、`project_item_state_changed`。  
理由：Project Board 是项目状态源，任何项目状态变化都必须被 Kernel 记录和恢复，否则 Project Board 与 Task Queue 会产生状态漂移。

### Web UI 后置

决定：项目管理先用 Markdown / JSON，Web UI 放到 Stage 3/4。  
理由：当前最值钱的是协议闭环，不是界面。过早做 UI 会拖慢 runtime 验证。

## 14.2 v1.x 设计决定

### Stage 0–4 已完成，后续不再自动进入 Stage 5

决定：Stage 0–4 视为完成范围，后续所有新增工作必须作为 optional track 单独批准。  
理由：继续以 Stage 递增会让项目无限膨胀，破坏封版边界。

### Real model integration 必须 Advisor-only 起步

决定：第一次真实模型接入只能用于 Advisor Preflight / Correction / Risk Scan。  
理由：真实模型直接执行任务会同时引入写入、成本、错误分类和安全边界问题。

### Context Pack v2 必须先于真实模型执行

决定：真实模型接入前必须定义最小上下文和显式检索协议。  
理由：避免 context rot，避免把完整项目状态直接塞进模型。

### Model Harness Profile 必须先于 provider routing

决定：每个模型必须有 profile，不只记录 provider/model_id。  
理由：不同模型有不同工具格式、prompt 风格、cache 行为和上下文边界。

### Unknown error 视为 harness bug

决定：UnknownHarnessBug 默认 fail hard。  
理由：unknown error 如果被普通 retry 吞掉，会污染上下文并掩盖 harness 问题。

### Routing experiments 仍然 observational

决定：RoutingExperimentReport 只能推荐，不自动改生产策略。  
理由：少量样本容易造成 routing 震荡。

### Skill extraction 不自动改 prompt

决定：Skill Extractor 只产出 SkillRecord，不自动注入 prompt 或修改 task spec。  
理由：经验沉淀需要人工或策略层批准后才能进入执行路径。

### Specialist agent 必须先有 role profile

决定：真实多 agent 前必须定义 planner / executor / debugger / verifier / advisor / integrator 的 input、output、forbidden behavior 和 quality metric。  
理由：否则 multi-agent 只是并发混乱，不是系统能力。

### Keep Rate 进入长期质量指标

决定：T4 draft PR 以后必须记录 Keep Rate。  
理由：测试通过不能说明输出长期有用；Keep Rate 才能反映代码是否被保留。

### 用户反馈进入质量闭环

决定：T2 之后应记录 post-action user feedback。  
理由：用户接受、修改、拒绝是质量信号，不能只看自动评分。

### 周期性维护 issue 是 post-closeout 的运行方式

决定：日志 review、unknown errors、eval regression、feedback、Keep Rate drop 应转成 maintenance issue。  
理由：harness 维护要有操作入口，不能只沉淀在 retrospective 中。

### Context Pack v2 四个 schema 必须完整定义

决定：`advisor_context_pack_v2`、`model_context_pack_v2`、`context_retrieval_request`、`context_retrieval_result` 是 T2 之前的前置协议，不能只作为名字占位。  
理由：真实模型接入前，必须明确上下文最小化和显式检索边界。

### `forbidden_previous_tools` 是强约束，不是提示语

决定：`forbidden_previous_tools` 是结构化禁用列表，由 Context Broker、Model Gateway、Advisor Broker 和 Evaluation Harness 共同执行。  
理由：模型切换时最容易延续旧工具格式；这必须在 harness 层阻断，而不是只写在 prompt 里。

### Keep Rate 与用户反馈 schema 只保留一个 canonical 来源

决定：`keep_rate_observation` 是 Keep Rate 的唯一 canonical 原始记录 schema；`user_feedback_event` 是用户反馈的唯一 canonical 原始记录 schema。  
理由：同一概念多套 schema 会造成实现歧义，后续 Track 必须有唯一权威结构。

### “应该先做”列表必须去重

决定：后续行动列表只保留唯一条目，不同时保留中文大小写或措辞不同的重复项。  
理由：post-closeout 文档用于执行决策，重复条目会制造优先级歧义。

### Final Gate 是最高准入函数，但不是目标来源

决定：Project Brief 是目标来源；Project Board 是状态事实源；Quality Gate 是质量 / 风险评估器；Final Gate 是最高准入函数；Memory / Optimization Plane 是经验沉淀层。  
理由：这能防止 Orchestrator、Advisor 或 Worker 把局部建议误当成全局目标重写，也能保持 `task completed ≠ project item done` 的核心不变量。

### Kernel 类型定位为 Orchestrator Kernel

决定：当前系统归类为 `Orchestrator Kernel with controlled adaptive-cognitive extensions`，不归类为完整 `Adaptive Cognitive Kernel`。  
理由：当前 Kernel 是 deterministic controller，智能能力、记忆、策略实验和反馈沉淀均位于外部受控组件；它没有自主目标重写、自动策略部署或真实模型执行权。

## 14.3 v1.3.x 设计决定

### v1.3.2 不替代 v1.2

v1.2 是封版状态说明，v1.3.2 是后续治理蓝图。

### Adaptive 发生在 harness policy sidecar，不发生在 runtime 自我授权

自适应的对象是 context pack、tool contract、routing rule、skill package、evaluation threshold、model profile。

### Governance & Policy Plane 是 v1.3.2 的核心新增层

没有 policy candidate lifecycle，系统不能声称 controlled adaptive。

### Evidence 必须分级

admitted evidence 才能参与 adoption；diagnostic evidence 只能生成候选。

### Cost-of-pass 是 efficient 的必要指标

没有 usage ledger 和 cost-of-pass，就不能证明 token-efficient improvement。

### Skills-first 优先于 specialist agents

先沉淀程序性技能，再考虑多 agent。多 agent 必须证明收益大于 token 和协调成本。

### Advisor 默认离线

Advisor 先作为 offline critic / evaluator，不进入 runtime 主路径。

### Controlled Adaptive 的最低门槛是 CA-6 / CA-7

只有 policy candidate lifecycle 和 governance approval path 都存在，系统才进入 Controlled Adaptive Orchestrator Kernel。

### Context Pack v2 的 canonical schema 仍来自 v1.2

v1.3.2 的五层结构是 composition layout，不替代 v1.2 的 advisor_context_pack_v2、model_context_pack_v2、context_retrieval_request、context_retrieval_result。

### rollback_plan 是 Policy Candidate Lifecycle 的必备结构

所有可部署 policy candidate 必须在 approval 前提供 rollback_plan。

### User-Style Mutation Eval 是 CA-3 gate

它不再只是 Track 顺序中的建议项，而是进入真实模型和 policy adoption 前的正式成熟度门槛。

### Track 9 以 Section 16 为 canonical 入场条件

Advisor-only Real Model Test 不能只按 Track 列表启动，必须满足真实模型接入前置条件。

### Track 顺序必须与 CA gate 顺序一致

User-Style Mutation Eval 是评估基底，应先于 Context Pack v2。Context Pack v2 的 offline tests 必须覆盖 formal_issue、user_style_chat_request、terse_ticket 三类输入，因此 Track 2 与 Track 3 的顺序固定为：先 User-Style Mutation Eval，再 Context Pack v2。

---

# 15. 附录

## 15.1 Project Board 状态枚举

```text
todo
ready
running
blocked
review
done
failed
```

## 15.2 Task Queue 状态枚举

```text
QUEUED
TRIAGED
READY
READY_READONLY
READY_WRITE
RUNNING
WAITING_APPROVAL
PAUSED_BUDGET
WAITING_DEPENDENCY
BLOCKED
BLOCKED_UPSTREAM_FAILED
BLOCKED_APPROVAL
BLOCKED_PROVIDER
COMPLETED
FAILED
CANCELLED_BY_DEPENDENCY
```

## 15.3 Artifact 状态枚举

```text
draft
produced
verified
rejected
promoted
accepted
superseded
archived
```

## 15.4 Final Gate 结果枚举

```text
pass
pass_with_notes
fail
requires_human_review
```

## 15.5 Quality Gate 结果枚举

```text
pass
pass_with_notes
fail_retryable
fail_terminal
requires_human_review
```

## 15.6 事件类型枚举

项目级事件：

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

任务 / 节点级事件：

```text
task_state_changed
node_started
node_completed
node_failed
approval_requested
approval_resolved
advisor_called
advisor_response_recorded
artifact_produced
artifact_verified
quality_gate_evaluated
final_gate_evaluated
checkpoint_created
checkpoint_restored
dag_mutated
sandbox_claim_created
sandbox_claim_released
supervisor_recovery_triggered
supervisor_recovery_completed
```

## 15.7 节点类型枚举

```text
project_architect
planner
architect
advisor
research_worker
code_scanner
builder
doc_writer
test_writer
refactor_worker
config_worker
verifier
reviewer
merge_worker
final_gate
quality_gate
artifact_gate
```

## 15.8 Project Dependency node_type 枚举

```text
requirement
module
test_case
bug
doc
integration
```

## 15.9 dependency_type 枚举

```text
hard_dependency
artifact_dependency
soft_dependency
approval_dependency
```

## 15.10 failure code 表

Canonical failure codes：

```text
F001_TIMEOUT
F002_BUDGET_EXCEEDED
F003_DEPENDENCY_FAILED
F004_APPROVAL_REJECTED
F005_PROVIDER_UNAVAILABLE
F006_SCOPE_VIOLATION
F007_TEST_FAILURE
F008_FORMAT_ERROR
F009_POLICY_VIOLATION
F010_CANCELLED
```

示例 subcodes：

```text
handoff_pack_incomplete
forbidden_file_touched
allowed_files_incomplete
line17_jsonl_concatenation
duplicate_event_id
provider_quota_exceeded
missing_required_artifact
context_pack_invalid
unknown_error_untriaged
```

## 15.11 fallback 路由表

```yaml
failure_code_to_fallback_strategy:
  F001_TIMEOUT:
    strategy: retry_or_split
    advisor_call: stuck
    human_review: false

  F002_BUDGET_EXCEEDED:
    strategy: pause_budget_or_reduce_scope
    advisor_call: risk_scan
    human_review: true

  F003_DEPENDENCY_FAILED:
    strategy: block_downstream
    advisor_call: none
    human_review: false

  F004_APPROVAL_REJECTED:
    strategy: cancel_or_replan
    advisor_call: correction
    human_review: true

  F005_PROVIDER_UNAVAILABLE:
    strategy: switch_provider_or_block
    advisor_call: none
    human_review: false

  F006_SCOPE_VIOLATION:
    strategy: block_and_scope_correction
    advisor_call: correction
    human_review: true

  F007_TEST_FAILURE:
    strategy: retry_fix_loop
    advisor_call: correction
    human_review: false

  F008_FORMAT_ERROR:
    strategy: validate_and_rewrite_artifact
    advisor_call: correction
    human_review: false

  F009_POLICY_VIOLATION:
    strategy: block_and_escalate
    advisor_call: risk_scan
    human_review: true

  F010_CANCELLED:
    strategy: stop_and_record
    advisor_call: none
    human_review: false
```

## 15.12 error_domain 枚举

```text
tool_contract_error
environment_error
context_error
model_judgment_error
evaluation_error
harness_bug
user_abort
provider_error
timeout
unknown_error
```

## 15.13 cost_of_pass_group 格式

`cost_of_pass_group` 定义一组可以被公平比较 cost-of-pass 的 eval rows。

推荐格式：

```text
<eval_suite>/<task_family>/<variant_family>/<success_criterion>
```

示例：

```text
harness_change_eval/doc_update/formal_issue/final_gate_pass
harness_change_eval/doc_update/user_style/final_gate_pass
real_world_read_only/bugfix/terse_ticket/quality_gate_pass
advisor_offline/config_rule/formal_issue/advisor_risk_scan_accepted
```

聚合规则：

```text
只能在同一 cost_of_pass_group 内比较 cost-of-pass。
不同 group 之间可以看趋势，但不能直接宣称 A 比 B 更高效。
cost_of_pass = group_total_estimated_cost / group_success_count。
如果 group_success_count = 0，则 cost_of_pass 为 undefined，并必须报告 failure。
```

## 15.14 CA gate classification

```text
CA-0 到 CA-2：adaptive-ready
CA-3 到 CA-5：evaluation-controlled adaptive preparation
CA-6 到 CA-7：Controlled Adaptive Orchestrator Kernel
CA-8：controlled real-model advisory mode
```

---

# 最终结论

本合并版把 v0.7.4.1 的 runtime / schema / Stage 0–4 基座，v1.2 的 post-closeout optional tracks 与 Cursor harness research，v1.2 addendum 的权责边界，以及 v1.3.2 的 Governance / Policy / CA gate 体系合并为一份可独立阅读的完整架构书。

当前系统的最终定位是：

```text
Orchestrator Kernel
with controlled adaptive-cognitive extensions
```

下一阶段目标是：

```text
Controlled Adaptive Orchestrator Kernel
```

但进入该状态的最低门槛不是写完架构书，而是达到：

```text
CA-6: Policy Candidate Lifecycle implemented
CA-7: Governance approval path enforced
```

在此之前，不应声称系统已经是 Controlled Adaptive Orchestrator Kernel。
