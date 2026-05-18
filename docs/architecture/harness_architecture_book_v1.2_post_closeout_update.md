# Token-Efficient Agent Harness 架构书 v1.2-post-closeout-update

版本：v1.2-post-closeout-update  
状态：基于 `v1.1-post-closeout-update` 的一致性修正版；统一 Keep Rate 与用户反馈 canonical schema，去除后续行动列表重复项，并保留 Cursor harness research 的全部补充内容。  
更新目的：把原本“Stage 0 启动前”的架构书，更新为“Stage 0–4 已完成、项目已封版、后续仅走可选 Track”的计划书。  
适用范围：用于后续维护、研究吸收、真实只读评估、未来可选真实模型/真实沙盒/生产化 Track 的边界定义。  

---

## 0. 与 v0.7.4.1-canonical 的关系

`v0.7.4.1-canonical` 仍然是底层协议和项目管理层设计的历史基准，尤其是：

```text
Project Management Plane
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
Policy Engine
Node Contract
Advisor Protocol
Stage 0 Runbook
```

但它已经不是当前项目状态说明书。原文定位是：

```text
Stage 0 启动前的架构参考文档
```

现在项目已经完成 Stage 0–4、完成 project closeout、发布到 GitHub、CI 通过，并完成第一轮真实风格只读评估扩展。因此，本文件正式覆盖原文中的这些内容：

```text
Stage 分层当前状态
当前实现路线
下一步建议
Stage 2 / Stage 3 / Stage 4 的实际完成边界
Post-closeout 维护模式
真实投入测试路线
Cursor harness research 带来的可选修改 Track
```

不覆盖的部分：

```text
底层 schema
事件协议
Project Board / Task Queue 设计
Project Architect 权限边界
Stage 0 的历史 runbook
设计决定中仍然有效的原则
```

---

## 1. 当前一句话定义

Token-Efficient Agent Harness 现在不是“准备进入 Stage 0 的架构设想”，而是一个已经完成 Stage 0–4 的：

```text
确定性本地 harness lab
+ 质量运行时
+ 受控智能 stub 层
+ 高级 runtime 抽象层
+ 真实风格只读评估 fixture suite
```

它的当前定位是：

> 一个已经封版的、本地可测试、可审计、无真实副作用的 token-efficient agent harness 实验仓库。它尚未接真实模型、真实 agent、真实 sandbox、真实并发 worker 或 Web UI。

---

## 2. 当前总状态

```text
Stage 0：完成
Stage 1：完成
Stage 2：完成
Stage 3：完成
Stage 4：完成
Project closeout：完成
GitHub private repo：已发布
CI：已通过
tag：stage0-4-complete 已推送
Real-World Read-Only Evaluation Track：第一版 + fixture expansion 已完成
当前模式：post-closeout maintenance
```

当前不处于任何 Stage 中间。

后续不应再自动创建 “Stage 5”。后续只能作为单独批准的 optional track 进行。

---

## 3. 更新后的核心原则

原 9 条核心原则继续有效，并新增以下 post-closeout 原则。

### 3.1 原核心原则继续有效

```text
1. 项目先拆成模块，模块再进入任务队列。
2. Project Board 是项目状态源，Task Queue 是执行队列。
3. 聪明模型负责项目抽象，便宜模型负责局部执行。
4. 模块必须有 Context Budget Contract。
5. 所有写入必须进入 sandbox / worktree。
6. Advisor 是短纠偏，不接管执行。
7. 质量由测试、规则、review 和 final gate 共同判断。
8. 无人值守必须可审批、可恢复、可解释。
9. Stage 0 先手动模拟，不急着做完整平台。
```

### 3.2 新增 post-closeout 原则

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

---

## 4. 已完成组件地图

### 4.1 Stage 0：Manual Project Simulation

完成内容：

```text
Project Brief
Project Board
Project Dependency Graph
Module Contracts
Test Case Packs
5 个手动任务
events.jsonl 草稿
completion.json
handoff_pack.json
run_log.md
batch_digest.md
retrospective
Stage 0 line 17 数据质量问题保留为 fixture
```

Stage 0 的价值：

```text
验证 Project Board 可以承载项目状态
验证 Project Dependency Graph 可以表达依赖
验证 Task Runtime schema 足够支撑真实小任务
验证 Batch Digest 可以作为无人值守后的主界面
```

---

### 4.2 Stage 1：Deterministic Local Runtime

已完成组件：

```text
Event Store
JSONL Validator
Projection Store
Project Board Manager
Task Queue Manager
Validator Suite
Batch Digest stub
Read-only CLI wrapper
Kernel
BatchRunner
TaskRecordStore
FinalGateRunner
Stage1Orchestrator
```

Stage 1 的边界：

```text
完成确定性本地 orchestration skeleton
不接模型
不跑真实 agent
不做真实 sandbox
不做并发 worker
不做 Web UI
```

核心不变量：

```text
task completed ≠ project item done
Project item 只有 Final Gate 之后才能 done
所有 event append 必须保持 JSONL 有效
docs/stage0/events.jsonl 永不修改
```

---

### 4.3 Stage 2：Quality Runtime

已完成组件：

```text
Scoring Engine
Artifact Gate
Quality Gate Manager
Evaluation Runner
Baseline Manager
Trajectory Monitor
Quality Digest Generator
```

Stage 2 的边界：

```text
规则化质量评估
无模型 judge
无真实任务执行
无真实 agent
无 routing optimizer 自动改策略
```

Stage 2 的价值：

```text
让 harness 能判断一次 run 是否好
让 fixture / task bundle / orchestrator flow 可以被评分
让质量门能区分 pass、pass_with_notes、fail_retryable、fail_terminal、requires_human_review
```

---

### 4.4 Stage 3：Controlled Intelligence Stubs

已完成组件：

```text
Advisor Broker
Advisor Protocol Validator
Model Gateway Stub
Model Capability Registry
Routing Experiment Manager
Controlled Model Evaluation Harness
Sampling Runner
Skill Extractor
Optional Orchestrator Advisor Hook
```

Stage 3 的边界：

```text
stub-first
无真实模型 API
无 API key
无 real provider
无真实 agent
无自动 routing adoption
无自动 prompt mutation
```

Stage 3 的价值：

```text
把未来真实模型接入点全部协议化
让 advisor、gateway、routing、sampling、skill extraction 都可以在无模型环境下被测试
把“真实模型”从架构风险降为 provider 替换风险
```

---

### 4.5 Stage 4：Advanced Runtime Abstractions

已完成组件：

```text
Dynamic DAG Manager
DAG Mutation Protocol
Sandbox Manager abstraction
File Claim System
Concurrency Controller
Runtime Supervisor
Checkpoint / Recovery Manager
Artifact Lifecycle Manager
Health Monitor
Dashboard Data Model
```

Stage 4 的边界：

```text
无真实 sandbox/process/container/VM execution
无真实并发 worker
无真实 Web UI
无真实 process recovery
无生产部署
所有高风险能力都是 abstraction + deterministic tests
```

Stage 4 的价值：

```text
定义了未来自治 runtime 所需的安全边界
定义了 DAG mutation、file claims、checkpoint、recovery、artifact lifecycle、health、dashboard 的数据模型和测试
但仍不产生真实副作用
```

---

## 5. Post-closeout 已完成事项

```text
GitHub private repo published
GitHub Actions CI added
AGENTS.md updated to post-closeout maintenance mode
README / ROADMAP / MODULE_MAP / TEST_MATRIX added
PROJECT_CLOSEOUT_REPORT added
stage0-4-complete tag pushed
Real-World Read-Only Evaluation Track first pass added
Real-World Read-Only Evaluation Fixture Expansion added
```

当前测试状态：

```text
350 tests OK
本地测试通过
GitHub Actions 通过
```

---

## 6. 当前真实测试状态

项目已经进入真实测试的第一层：**真实风格只读评估**。

### 6.1 已完成：Synthetic Real-World Read-Only Fixtures

已有 fixture 类型：

```text
project-alpha
doc-update-project
bugfix-project
config-rule-project
failure-fix-loop-project
cross-task-dependency-project
```

这些 fixture 验证：

```text
TaskRecordStore
FinalGateRunner
ScoringEngine
ArtifactGate
QualityGate
Batch Digest
Projection replay
Cross-task dependency projection
Canonical failure_code + freeform failure_subcode
Read-only fixture safety
```

### 6.2 尚未开始：真实项目复制 fixture

下一层真实测试不是接模型，而是：

```text
从真实项目复制 2–3 个脱敏任务样本到 tests/fixtures/real_world_eval/
只读运行现有 harness
不修改真实项目
不执行命令
不调用模型
```

---

## 7. 真实投入测试路线

### T0：Local deterministic tests

状态：完成。

```text
350 tests OK
CI 通过
```

### T1：Real-world read-only evaluation

状态：已启动，第一版完成。

继续条件：

```text
增加 2–3 个真实复制 fixture
确认 schema、digest、score、quality gate 在真实形态下不崩
```

### T2：Advisor-only real model test

状态：未开始。

允许行为：

```text
真实模型只做 Advisor Preflight / Correction / Risk Scan
只返回 advisor_response
只记录结果
不修改文件
不执行命令
不进入 sandbox
不提交 PR
```

必须有：

```text
token/cost 上限
provider profile
advisor_context_pack v2
before/after quality comparison
human review
```

### T3：Disposable repo sandbox test

状态：未开始。

允许行为：

```text
只在 disposable repo copy 中测试
验证 file claims / artifact export / checkpoint / recovery
不碰主仓库
不自动提交
```

### T4：低风险真实任务 draft PR

状态：未开始。

允许行为：

```text
低风险 repo
单个 issue
人工 review
最多 draft PR
无自动 merge
```

### T5：夜间 batch 测试

状态：未开始。

允许行为：

```text
3–5 个低风险任务
严格预算
早上人工验收 Batch Digest
无自动 merge
```

---

## 8. Cursor Harness Research 带来的修改选项

Cursor 团队关于持续改进 agent harness 的文章给本项目带来的核心修正是：

> 后续重点不是直接换模型，而是持续改进 harness 的评估、上下文、工具错误、模型 profile、role profile 和真实使用反馈闭环。

以下内容是可选修改 Track，不是 Stage 5。

---

### 8.1 Harness Change Evaluation Track

目的：

```text
给每次 harness 改动建立 before/after 对比机制。
```

应记录：

```text
test pass count
fixture score
quality digest diff
event count
tool/error count
latency placeholder
token/cost placeholder
baseline delta
keep_rate placeholder
post_action_user_feedback placeholder
```

Keep Rate 概念占位：

```text
8.1 只声明 Harness Change Evaluation 需要记录 Keep Rate 这一类长期质量信号。
实现时不得在 8.1 自行发明 schema。
canonical 原始记录 schema 见 8.7.1：keep_rate_observation。
若需要汇总指标，例如按 task / pull request 统计 retained_ratio，应从 keep_rate_observation 派生，不作为新的 canonical 写入 schema。
```

说明：Keep Rate 不等于测试通过率。测试通过说明输出当下可接受；Keep Rate 追踪输出在真实代码库里是否被保留，回答“这段输出后来是否真的有用”。在 T4 draft PR 和未来真实任务 track 中，Keep Rate 必须作为长期质量指标占位。

Post-action 用户反馈概念占位：

```text
8.1 只声明 Harness Change Evaluation 需要记录 post-action 用户反馈这一类质量信号。
实现时不得在 8.1 自行发明 schema。
canonical 原始记录 schema 见 8.7.2：user_feedback_event。
字段名以 8.7.2 为准：satisfaction_label、linked_event_ids 等必须使用 canonical 名称。
```

说明：v1.2 只定义反馈记录结构，不引入 LLM judge。未来如果引入 LLM judge，只能用于读取用户反馈语义，不得替代测试、Artifact Gate 或人工审批。

不做：

```text
不改 runtime 行为
不接模型
不自动优化 routing
```

交付物：

```text
docs/HARNESS_CHANGE_EVAL.md
tests/test_harness_change_eval.py
evaluation report format
```

---

### 8.2 Tool/Error Taxonomy Hardening Track

目的：

```text
把工具错误和 harness bug 更严格地区分，避免 unknown error 污染上下文。
```

建议新增分类：

```text
InvalidArguments
UnexpectedEnvironment
ProviderError
UserAborted
Timeout
UnknownHarnessBug
```

规则：

```text
UnknownHarnessBug 必须 fail hard
不能被归类为普通 retryable failure
tool error 必须能进入 quality digest 和 trajectory report
```

交付物：

```text
docs/tool_error_taxonomy.md
error taxonomy tests
quality digest error section
```

---

### 8.3 Context Pack v2 Track

目的：

```text
从“全量上下文 upfront”改成“最小上下文 + 显式检索”。
```

原则：

```text
默认传 task_spec、status、artifact_refs、quality warnings
不默认传完整 artifact 正文
run_log / diff / evidence 通过显式 read API 获取
Context Pack 必须有 token budget
```

#### 8.3.1 advisor_context_pack_v2

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

规则：

```text
advisor_context_pack_v2 只给 Advisor 判断所需的最小材料。
默认不嵌入完整 diff、完整 artifact、完整 run_log。
Advisor 如需更多材料，必须生成 context_retrieval_request。
```

#### 8.3.2 model_context_pack_v2

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

规则：

```text
model_context_pack_v2 是给未来真实模型执行或建议的上下文包。
它必须引用 Model Harness Profile。
如果发生模型切换，必须携带 switch_context，明确旧模型可用但新模型禁止的工具。
```

#### 8.3.3 context_retrieval_request

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

规则：

```text
retrieval request 必须说明 reason。
默认只能请求 summary 或 excerpt。
full 内容请求必须被 budget 和 policy 明确允许。
```

#### 8.3.4 context_retrieval_result

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

规则：

```text
retrieval result 必须记录 token estimate 和 content_mode。
denied / budget_exceeded 是正常结果，不是 uncontrolled error。
```

不做：

```text
不立即接真实模型
不改已有 runtime flow
不允许模型绕过 retrieval policy 读取任意文件
```

---

### 8.4 Model Harness Profile Track

目的：

```text
给每个 model tier 定义 harness profile，而不是只按 provider/model_id 路由。
```

建议字段：

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

`forbidden_previous_tools` 语义：

```text
它不是自然语言提醒，而是模型切换时的强约束列表。
字段可以按 tool_id 精确禁用，也可以按 tool_type 禁用。
主要用于防止新模型延续旧模型的工具调用格式、旧 patch 格式或旧 retrieval 模式。
```

执行责任：

```text
Context Broker / prompt assembly：不得把 forbidden_previous_tools 描述为可用工具。
Model Gateway：在未来真实 provider 调用前校验 requested tool 不在 forbidden_previous_tools。
Advisor Broker：risk_scan 时可以把 forbidden_previous_tools 违规列为 risk。
Verifier / Evaluation Harness：把违规工具调用记录为 Tool/Error Taxonomy 中的 InvalidArguments 或 ModelProfileViolation。
```

必须解决：

```text
中途切换模型时的工具集不兼容
历史上下文 out-of-distribution
cache miss 成本增加
旧工具调用格式污染新模型
```

---

### 8.5 Specialist Agent Role Profiles Track

目的：

```text
先定义 specialist roles，再允许真实多 agent。
```

建议角色：

```text
planner
executor
debugger
verifier
advisor
integrator
```

每个 role 必须有：

```text
input contract
output contract
forbidden behavior
quality metric
context budget
tool access
handoff requirements
```

不做：

```text
不启动真实多 agent 并发
不执行真实任务
```

---

### 8.6 Advisor-only Real Model Test Track

目的：

```text
作为最早的真实模型接入 Track，只让模型当顾问，不让模型执行。
```

允许：

```text
Advisor Preflight
Advisor Correction
Advisor Risk Scan
advisor_response 记录
quality/digest 对比
```

禁止：

```text
文件修改
shell command
sandbox execution
PR 创建
自动审批
自动 merge
```

必须前置：

```text
Context Pack v2 reviewed
Model Harness Profile reviewed
token/cost budget set
provider credentials handled outside repo
human approval
```

---

---

### 8.7 Outcome Feedback and Maintenance Loop Track

目的：

```text
把长期结果信号、用户反馈和周期性 harness 维护 issue 变成正式流程。
```

包含三个子机制：

```text
1. Keep Rate Tracking
2. Post-action User Feedback Loop
3. Periodic Harness Log Review → Maintenance Issue Generation
```

#### 8.7.1 Keep Rate Tracking

Canonical 关系：

```text
keep_rate_observation 是唯一 canonical 原始记录 schema。
8.1 中提到的 keep_rate_metric 只是概念性/汇总性指标名称，不作为实现 schema。
任何 task / artifact / pull request 级 Keep Rate 汇总，都必须由一组 keep_rate_observation 派生。
```

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

规则：

```text
T4 draft PR 之前只定义占位，不自动扫描真实 repo。
进入真实 PR 测试后，Keep Rate 应成为核心长期指标。
Keep Rate 不能替代测试，但能补充测试无法回答的“是否被长期保留”。
```

#### 8.7.2 Post-action User Feedback Loop

Canonical 关系：

```text
user_feedback_event 是唯一 canonical 原始记录 schema。
8.1 中提到的 post_action_user_feedback 是概念性名称，不作为实现 schema。
实现字段名以本节为准：使用 satisfaction_label，不使用 semantic_satisfaction_label；必须保留 linked_event_ids 以便把用户反馈关联回 advisor / final_gate / artifact / event 证据。
```

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

规则：

```text
T2 之后允许记录用户反馈。
LLM judge 只能作为未来可选 label_source，不能在当前阶段自动决定质量门。
用户反馈应进入 Harness Change Evaluation 和 Quality Digest 的报告层。
```

#### 8.7.3 Periodic Harness Maintenance Loop

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

规则：

```text
周期性 review 不直接改代码。
它只把日志、unknown errors、eval regression、用户反馈和 Keep Rate 下降转成维护 issue。
每个维护 issue 需要人工批准后才能进入实现 track。
```

交付物：

```text
docs/harness_maintenance_loop.md
maintenance issue schema
read-only report generator
```

## 9. 推荐后续顺序与 Track 入场条件

当前不要继续加 runtime 功能，也不要马上接真实模型。

推荐顺序：

```text
1. 人工 review 当前 README / ROADMAP / MODULE_MAP / TEST_MATRIX
2. 增加 2–3 个真实复制 read-only fixtures
3. Harness Change Evaluation Track
4. Tool/Error Taxonomy Hardening Track
5. Outcome Feedback and Maintenance Loop Track
6. Context Pack v2 Track
7. Model Harness Profile Track
8. Specialist Agent Role Profiles Track
9. Advisor-only Real Model Test Track
```

如果目标是尽快真实投入测试：

```text
先做 T1 扩展：真实复制 fixture
再做 T2：Advisor-only real model
不要跳到 T3/T4/T5
```

### 9.1 通用入场条件

任何可选 Track 开始前必须满足：

```text
Git working tree clean
CI passing
不修改 docs/stage0/events.jsonl
有明确 human approval
有单独 branch
有 stop conditions
不默认创建 Stage 5
```

### 9.2 各 Track 入场条件

| Track | 入场条件 | 禁止事项 | 退出产物 |
|---|---|---|---|
| 人工文档 review | Stage 0–4 closeout 完成，CI 绿 | 不改 runtime | review notes / docs patch |
| 真实复制 read-only fixtures | 人工提供脱敏 fixture，确认无 secrets | 不读写真实项目，不执行命令 | fixture tests + eval report |
| Harness Change Evaluation | 固定 eval suite 已存在，baseline 可复现 | 不自动修改 routing，不接模型 | before/after report schema + tests |
| Tool/Error Taxonomy Hardening | 已收集至少一批 tool/error 样例或现有 failure gaps | 不吞 unknown，不把 unknown 当 retryable | taxonomy doc + validator tests |
| Outcome Feedback and Maintenance Loop | read-only eval 和 digest 稳定，至少有真实/拟真反馈来源 | 不让反馈自动改代码，不让 LLM judge 决策质量门 | feedback / keep_rate / maintenance issue schema |
| Context Pack v2 | Tool/Error Taxonomy 已确认，目标模型使用场景明确 | 不接真实模型，不塞全量上下文 | 4 个 context schema + retrieval policy |
| Model Harness Profile | Context Pack v2 已 review，model tier 列表明确 | 不配置真实 credentials，不调用 provider | profile schema + switch rules |
| Specialist Agent Role Profiles | Model Harness Profile 已 review，角色边界明确 | 不启动真实多 agent，不并发执行 | role profile docs + contract tests |
| Advisor-only Real Model Test | T1 fixture 扩展稳定，Context Pack v2 和 Model Harness Profile 已 review，token/cost budget 已批准 | 不修改文件，不执行命令，不进 sandbox，不自动 PR | advisor_response logs + quality/digest comparison |

## 10. 更新后的“不要先做”

```text
不要创建 Stage 5
不要直接接真实模型执行任务
不要让模型修改文件
不要让模型执行 shell
不要打开真实 sandbox execution
不要启动真实并发 worker
不要自动生成 PR
不要自动 merge
不要自动采用 routing 实验结果
不要让 Skill Extractor 自动改 prompt
不要把 UI/dashboard 作为下一步
```

---

## 11. 更新后的“应该先做”

```text
维护 current repo stability
继续 read-only real-world evaluation
建立 Harness Change Evaluation
硬化 Tool/Error Taxonomy
建立 Outcome Feedback and Maintenance Loop
设计 Context Pack v2
设计 Model Harness Profile
设计 Specialist Agent Role Profiles
最后才做 Advisor-only real model test
```

---

## 12. 更新后的设计决定记录

### 12.1 Stage 0–4 已完成，后续不再自动进入 Stage 5

决定：Stage 0–4 是当前任务书的顶层完成范围。  
理由：Stage 4 已经覆盖高级 runtime abstraction；继续扩展必须作为独立 track，而不是自然 Stage 5。

### 12.2 Real model integration 必须 Advisor-only 起步

决定：第一次真实模型接入只允许 Advisor Preflight / Correction / Risk Scan。  
理由：Advisor 不执行动作，风险最低，最适合作为真实模型接入的第一层。

### 12.3 Context Pack v2 必须先于真实模型执行

决定：真实模型参与执行前，必须先定义最小上下文与显式检索协议。  
理由：全量上下文会造成 context rot、token 浪费和错误传播。

### 12.4 Model Harness Profile 必须先于 provider routing

决定：每个模型必须有 harness profile，不能只用 tier/provider/model_id。  
理由：不同模型的 tool format、prompt style、cache、context 行为不同，直接切换会造成工具不兼容和上下文漂移。

### 12.5 Unknown error 视为 harness bug

决定：未知工具错误或未知 harness 错误不得静默 retry。  
理由：unknown error 会污染上下文并掩盖 harness 本身缺陷。

### 12.6 Routing experiments 仍然 observational

决定：routing 实验结果只能生成 recommendation，不能自动应用。  
理由：自动 routing 变更会破坏可解释性和可回滚性。

### 12.7 Skill extraction 不自动改 prompt

决定：Skill Extractor 只生成可检索知识，不自动修改 prompts。  
理由：自动 prompt mutation 会引入难以审计的行为变化。

### 12.8 Specialist agent 必须先有 role profile

决定：任何真实多 agent 执行前，必须先定义 planner/executor/debugger/verifier/advisor 等角色 profile。  
理由：没有 role contract 的多 agent 只会放大混乱。

---

### 12.9 Keep Rate 进入长期质量指标

决定：T4 draft PR 及以后必须记录 Keep Rate 占位或实际观测。  
理由：测试通过率只能说明当下可运行，Keep Rate 才能回答输出是否被真实代码库长期保留。

### 12.10 用户反馈进入质量闭环

决定：T2 以后应记录 post-action user feedback，但不让它自动控制质量门。  
理由：用户接受、修改、拒绝和请求修改是重要语义质量信号，但必须先作为观测信号，不应直接驱动自动执行。

### 12.11 周期性维护 issue 是 post-closeout 的运行方式

决定：日志 review、unknown errors、eval regression、feedback 和 Keep Rate 下降应生成维护 issue，而不是直接改 runtime。  
理由：post-closeout 模式需要可审计维护闭环，不能回到无边界开发。

### 12.12 Context Pack v2 四个 schema 必须完整定义

决定：`advisor_context_pack_v2`、`model_context_pack_v2`、`context_retrieval_request`、`context_retrieval_result` 必须在真实模型测试前完成字段级定义。  
理由：没有字段级协议，真实模型接入会重新退化成“把上下文全塞进去”的不稳定模式。

### 12.13 `forbidden_previous_tools` 是强约束，不是提示语

决定：`forbidden_previous_tools` 必须是结构化列表，并由 Context Broker / Model Gateway / Evaluation Harness 共同执行。  
理由：模型切换时，旧工具格式污染新模型是高风险问题，仅靠 prompt 提醒不可靠。

### 12.14 Keep Rate 与用户反馈 schema 只保留一个 canonical 来源

决定：`keep_rate_observation` 和 `user_feedback_event` 是 canonical 原始记录 schema；8.1 中的 Keep Rate / post-action feedback 只作为 Harness Change Evaluation 的概念性指标入口。  
理由：避免同一概念在文档中出现两套字段，导致实现者不知道该用哪个 schema。

### 12.15 “应该先做”列表必须去重

决定：后续行动列表只保留唯一条目，不同时保留中文大小写或措辞不同的重复项。  
理由：post-closeout 文档用于执行决策，重复条目会制造优先级歧义。

## 13. 与原文哪些章节冲突或过时

### 13.1 原第 9 节 Stage 分层已过时

原文把 Stage 0–4 定义为未来路线。现在这些 Stage 已经完成。保留原定义作为历史背景，但当前状态以本文为准。

### 13.2 原第 11 节“当前实现路线”已过时

原文建议“进入 Stage 0，先跑 5 个任务”。现在 Stage 0–4 已完成，当前路线是 post-closeout optional tracks。

### 13.3 原第 13 节“下一步进入 Stage 0”已过时

当前下一步不是 Stage 0，而是：

```text
review docs
expand real-world read-only fixtures if needed
run approved optional modification tracks
```

---

## 14. 当前最终结论

Token-Efficient Agent Harness 已经从 v0.7.4.1 的“Stage 0 启动前架构书”，推进为一个完成 Stage 0–4 的 post-closeout harness lab。

现在最重要的不是继续堆功能，而是：

```text
稳定测试
保持边界
扩大真实只读评估
补齐 evaluation / error taxonomy / outcome feedback / context / model profile
补上 Keep Rate、用户反馈和周期性维护 issue 机制
最后再做 Advisor-only real model test
```

一句话：

> 当前项目已经可以作为 token-efficient agent harness 的可测试实验基座。下一步真实投入应从 read-only evaluation、harness change evaluation、context/profile schema 和 Advisor-only real model test 开始，而不是直接让模型执行任务。
