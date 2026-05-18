# Token-Efficient Agent Harness 架构书 v1.3.1 — Controlled Adaptive Orchestrator Kernel Blueprint

版本：v1.3.1-controlled-adaptive-orchestrator  
状态：基于 v1.3 的一致性修正版；修复 Context Pack v2 与 v1.2 schema 的关系、rollback_plan 占位、User-Style Mutation Eval gate、cost_of_pass_group 语义、Track 9 入场条件引用。  
定位：不替代 v1.2 的封版状态说明；本文件定义从 Orchestrator Kernel 走向 Controlled Adaptive Orchestrator Kernel 的治理路线、成熟度门槛和 Track 边界。  
适用范围：post-closeout optional tracks、evaluation-first adaptation、policy candidate lifecycle、真实模型接入前置条件。  
非适用范围：真实自治执行、Stage 5、无人审批自修改、自动策略部署、真实 sandbox 执行、真实多 agent 并发。

---

## 0. 与现有架构文件的关系

当前架构文件分层如下：

```text
v0.7.4.1-canonical
  = Stage 0 启动前的历史架构基准。

v1.2-post-closeout-update
  = Stage 0–4 完成后的封版状态说明与 optional track 清单。

v1.2-authority-kernel-addendum
  = 权责边界与 Kernel 类型判定。

v1.3.1-controlled-adaptive-orchestrator
  = 从 Orchestrator Kernel 走向 Controlled Adaptive Orchestrator Kernel 的治理蓝图。
```

本文件不推翻 v1.2。v1.2 仍是当前完成状态的基准；v1.3.1 只规定下一组可选 Track 如何安全推进。

---

## 1. 当前系统定位

当前系统仍然是：

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

因此，下一步不是提升 runtime 自治，而是建立受控自适应治理闭环。

---

## 2. Controlled Adaptive Orchestrator Kernel 定义

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

---

## 3. 权责边界保持不变

v1.2 addendum 的权责边界继续作为 canonical 定义：

```text
Project Brief = 目标来源
Project Board = 状态事实源
Quality Gate = 质量 / 风险评估器
Final Gate = 最高准入函数
Memory / Optimization Plane = 经验沉淀层
Orchestrator = deterministic coordination / state progression controller
```

v1.3.1 新增约束：

```text
Governance & Policy Plane = 策略候选、证据准入、审批、回滚与部署范围控制层
Evaluation Plane = 判断候选改动是否可被采纳的证据层
Memory / Optimization Plane = 沉淀经验，但不自动部署策略
Orchestrator Core = 执行状态推进，但不执行自我修改
```

如果这些层发生冲突，优先级如下：

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

## 4. Plane 架构更新

### 4.1 Orchestrator Core

职责：

```text
validate event log
replay projections
select ready work
advance task/project state
call gates
write events
produce deterministic outputs
```

包括：

```text
EventStore
ProjectionStore
Kernel
BatchRunner
TaskRecordStore
FinalGateRunner
Stage1Orchestrator
```

禁止：

```text
自动改目标
自动写长期记忆
自动采纳 routing policy
自动修改 prompt / skill / context pack
直接调用真实模型执行任务
```

### 4.2 Evaluation Plane

职责：判断候选改动是否变好、变差或不可判定。

包括：

```text
Real-World Read-Only Evaluation
Harness Change Evaluation
Fixture Expansion
Quality Gate
Scoring Engine
Quality Digest
Baseline Manager
Trajectory Monitor
Future User-Style Mutation Eval
Future Usage Ledger / Cost-of-Pass Eval
```

输入：

```text
fixed deterministic fixtures
realistic read-only fixtures
user-style mutation fixtures
harness change snapshots
quality digest
final gate results
usage ledger
manual review notes
```

输出：

```text
admitted evidence
diagnostic evidence
regression signal
cost / score / quality delta
candidate acceptance recommendation
```

### 4.3 Memory / Optimization Plane

职责：沉淀事实、经验、反馈、失败模式和候选线索。

包括：

```text
run logs
eval records
retrospectives
skill records
baseline records
keep_rate_observation
user_feedback_event
failure clusters
policy candidate evidence refs
```

禁止：

```text
不直接改 runtime
不直接改 Project Brief
不直接部署 policy
不把一次失败自动写成永久规则
不让 LLM critique 成为未经 review 的 canonical memory
```

### 4.4 Governance & Policy Plane

v1.3.1 新增核心 Plane。

职责：把经验转化为候选策略，并控制其评估、审批、部署和回滚。

包括：

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

它不是 agent，不执行任务，不拥有目标。它只回答：

```text
这个候选改动是否有足够证据？
证据是否可被准入？
是否通过固定评估？
是否需要人工审批？
如果失败如何回滚？
允许部署到哪里？
```

---

## 5. Evaluation Admission Contract

v1.3.1 必须把证据分为 admitted evidence 和 diagnostic evidence。

### 5.1 Admitted Evidence

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

### 5.2 Diagnostic Evidence

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

## 6. Policy Candidate Lifecycle

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

### 6.1 policy_candidate_manifest

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

### 6.2 candidate_evidence_pack

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

### 6.3 approval_record

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

### 6.4 policy_registry_entry

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

### 6.5 rollback_plan

`rollback_plan` 是 Policy Candidate Lifecycle 的核心结构，不再是引用占位。

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

规则：

```text
rollback_plan 必须在 policy adoption 前存在。
rollback_plan 的执行对象只限 harness-level 改动，不允许回滚真实用户项目文件。
rollback 默认由 human-approved maintainer 或显式授权工具执行；Orchestrator 不自动执行 rollback。
如果 rollback 失败，policy_registry_entry 必须进入 rolled_back 或 failed_review 状态并要求人工处理。
```

---

## 7. Tool/Error Taxonomy Hardening

Deep Research 的最高优先级建议是先硬化工具/错误分类，因为错误归因不清会污染所有 adaptive signal。

v1.3.1 推荐 error domains：

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

每个 error record 至少应包含：

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

关键规则：

```text
unknown_error 不能被当成普通 retryable failure。
unknown_error 必须进入 human triage。
未经归类的 unknown_error 不能驱动 policy candidate adoption。
```

---

## 8. Context Pack v2 and Memory Boundary

### 8.1 与 v1.2 四个 Context Pack schema 的关系

v1.2 的四个 schema 仍然是 canonical wire schemas：

```text
advisor_context_pack_v2
model_context_pack_v2
context_retrieval_request
context_retrieval_result
```

v1.3.1 第 8 节不替代这四个 schema。v1.3.1 的五层结构是它们之上的 **composition layout / semantic layering**，用于规定 pack 内容如何组织、裁剪、缓存、检索和写入 memory digest。

关系如下：

```text
advisor_context_pack_v2
  -> 使用五层结构组织 advisor 所需的最小上下文

model_context_pack_v2
  -> 使用五层结构组织 model / role 所需的最小上下文

context_retrieval_request
  -> 五层结构中 dynamic_refs / memory_digest / recent_evidence 的显式检索入口

context_retrieval_result
  -> 显式检索后的返回结构，必须记录 content_mode、token estimate、budget impact
```

实现规则：

```text
v1.2 四 schema = 对外协议 / wire format canonical source。
v1.3.1 五层结构 = 对内编排布局 / pack compiler target。
不得另行创建第五套 context_pack schema。
如需新增字段，应作为 v1.2 advisor_context_pack_v2 / model_context_pack_v2 的 extension fields 或 nested context_layers，而不是替换原 schema。
```

建议新增嵌套字段：

```yaml
context_layers:
  invariants:
  task_pack:
  dynamic_refs:
  memory_digest:
  recent_evidence:
```

### 8.2 五层结构

Context Pack v2 不应是更大的 prompt，而应是可测试的上下文编排协议。

v1.3.1 建议五层结构：

```text
invariants
  长生命周期：项目不变量、系统规则、质量门槛

task_pack
  中生命周期：当前任务目标、限制、成功标准

dynamic_refs
  短生命周期：文件路径、证据 refs、检索指针

memory_digest
  中长生命周期：历史决策摘要、已验证结论、未决问题

recent_evidence
  极短生命周期：最近 tool_result、diff、失败诊断
```

必备编排字段：

```text
pack_id
schema_version
context_budget
cache_policy
freshness
pack_prune_policy
conflict_resolution
drop_reason
evidence_refs
memory_digest_refs
retrieval_policy
context_layers
```

关键规则：

```text
默认最小上下文。
完整内容通过 explicit retrieval 获取。
memory_digest 必须有来源、过期策略和冲突处理规则。
Context Pack v2 必须与 usage ledger 联动。
```

Memory Boundary：

```text
EventStore / TaskRecordStore 保存事实。
Memory / Optimization Plane 保存经验和摘要。
Context Pack Builder 只读取经过允许的 refs。
Orchestrator 不直接写长期记忆。
Skill Extractor 不自动修改 prompt。
```

---

## 9. Usage Ledger / Cost-of-Pass Track

项目名里的 efficient 必须进入指标，而不是停留在口号。

每条 eval row 应记录：

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

### 9.1 cost_of_pass_group 语义

`cost_of_pass_group` 定义一组可以被公平比较 cost-of-pass 的 eval rows。

它不是模型 tier，也不是 fixture 名称本身。它必须绑定同一类任务分布、同一成功标准和同一评估入口。

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

规则：

```text
先达到质量阈值，再优化成本。
没有 usage ledger，不允许启用 routing optimization。
没有 cost-of-pass，不允许宣称 token-efficient improvement。
```

---

## 10. Realistic User-Style Mutation Eval

v1.3.1 需要补上 user-style mutation eval，以避免只对 formal issue 文本过拟合。

每个重要 fixture 应尽量有三种表达：

```text
formal_issue
user_style_chat_request
terse_ticket
```

建议元数据：

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

规则：

```text
固定 fixture 用于回归。
user-style mutation 用于抗格式偏差。
realistic read-only slice 用于贴近真实任务形态。
三者都不显著恶化，候选策略才可进入 approval。
```

---

## 11. Model Profiles and Shadow Routing

当前不应启用 active multi-model routing。应先建立 model-specific harness profile 和 shadow routing。

model_harness_profile 至少包含：

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

Shadow routing 规则：

```text
shadow router 可以推荐，不可执行。
shadow router 的建议属于 diagnostic evidence。
只有经过固定 fixture、user-style mutation、realistic read-only 三联评估后，才能提交 policy candidate。
active routing 需要 human approval。
```

---

## 12. Skills-First Policy

v1.3.1 采用：

```text
skills first, specialists later
```

优先封装为 skill 的内容：

```text
triage / taxonomy labeling
fixture mutation
eval authoring / admission checking
report normalization
context pack assembly rules
error classification rules
```

只有满足以下条件，才考虑 specialist agent：

```text
任务是 read-heavy
边界清晰
可并行
不会写同一文件区域
有单 agent + skills baseline
能证明收益大于 token / coordination 成本
```

禁止：

```text
不允许 write-heavy specialist agents 直接并发写主仓库。
不允许 skill extraction 自动改 prompt。
不允许 subagent 绕过 Orchestrator Core 或 Final Gate。
```

---

## 13. Advisor 策略更新

v1.3.1 将 Advisor 默认定位为：

```text
offline evaluator / critic / candidate analyst
```

而不是 runtime 常驻执行角色。

推荐顺序：

```text
1. No-advisor baseline
2. Advisor-only offline critique
3. Advisor-only real model test
4. Advisor in runtime path — 当前不推荐
```

Advisor 输出默认属于 diagnostic evidence，除非经过固定 eval、manual review 和 Governance & Policy Plane 准入。

Advisor 不允许：

```text
直接修改文件
直接执行 shell
直接进入 sandbox
直接通过 Final Gate
直接部署 policy
```

---

## 14. Controlled Adaptive Maturity Gates

v1.3.1 用 CA gates 判断系统是否真的进入 controlled adaptive，而不是只写了文档。

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
  v1.2 canonical schemas remain intact; v1.3.1 context_layers mapping, prune policy, retrieval policy, and memory boundary are tested offline.

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

## 15. Track 顺序更新

v1.3.1 推荐下一组 Track：

```text
1. Tool/Error Taxonomy Hardening
2. Context Pack v2 and Memory Boundary
3. Realistic User-Style Mutation Eval
4. Usage Ledger / Cost-of-Pass Track
5. Model Profiles and Shadow Routing
6. Skills Registry and Skills-First Policy
7. Policy Candidate Lifecycle
8. Advisor-Only Offline Critique
9. Advisor-Only Real Model Test — 必须满足 Section 16 全部前置条件
```

不推荐跳过前 7 项直接进入真实模型测试。

Track 9 以 Section 16 为唯一 canonical 入场条件。若 Section 15 与 Section 16 出现冲突，以 Section 16 为准。

---

## 16. 真实模型接入前置条件

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

## 17. v1.3.1 明确禁止事项

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
```

---

## 18. 设计决定记录

### 18.1 v1.3.1 不替代 v1.2

v1.2 是封版状态说明，v1.3.1 是后续治理蓝图。

### 18.2 Adaptive 发生在 harness policy sidecar，不发生在 runtime 自我授权

自适应的对象是 context pack、tool contract、routing rule、skill package、evaluation threshold、model profile。

### 18.3 Governance & Policy Plane 是 v1.3.1 的核心新增层

没有 policy candidate lifecycle，系统不能声称 controlled adaptive。

### 18.4 Evidence 必须分级

admitted evidence 才能参与 adoption；diagnostic evidence 只能生成候选。

### 18.5 Cost-of-pass 是 efficient 的必要指标

没有 usage ledger 和 cost-of-pass，就不能证明 token-efficient improvement。

### 18.6 Skills-first 优先于 specialist agents

先沉淀程序性技能，再考虑多 agent。多 agent 必须证明收益大于 token 和协调成本。

### 18.7 Advisor 默认离线

Advisor 先作为 offline critic / evaluator，不进入 runtime 主路径。

### 18.8 Controlled Adaptive 的最低门槛是 CA-6 / CA-7

只有 policy candidate lifecycle 和 governance approval path 都存在，系统才进入 Controlled Adaptive Orchestrator Kernel。

### 18.9 Context Pack v2 的 canonical schema 仍来自 v1.2

v1.3.1 的五层结构是 composition layout，不替代 v1.2 的 advisor_context_pack_v2、model_context_pack_v2、context_retrieval_request、context_retrieval_result。

### 18.10 rollback_plan 是 Policy Candidate Lifecycle 的必备结构

所有可部署 policy candidate 必须在 approval 前提供 rollback_plan。

### 18.11 User-Style Mutation Eval 是 CA-3 gate

它不再只是 Track 顺序中的建议项，而是进入真实模型和 policy adoption 前的正式成熟度门槛。

### 18.12 Track 9 以 Section 16 为 canonical 入场条件

Advisor-only Real Model Test 不能只按 Track 列表启动，必须满足 Section 16 全部前置条件。

---

## 19. 下一步执行建议

v1.3.1 之后，不要继续改总架构书。下一步应该落地第一条 Track：

```text
Tool/Error Taxonomy Hardening Track
```

目标：

```text
定义 docs/tool_error_taxonomy.md
新增 tests/test_error_taxonomy.py
新增 tests/fixtures/tool_error_cases/
让 unknown_error、context_error、tool_contract_error、environment_error、model_judgment_error 可被区分
```

边界：

```text
不改 runtime 主路径
不接真实模型
不执行任务
不进 Stage 5
```

---

## 20. 最终结论

当前项目不需要推翻 Orchestrator Kernel，也不应该直接迈向真实自治执行。

v1.3.1 的核心结论是：

```text
保持 deterministic Orchestrator Core。
把 adaptive 能力限制在 harness policy sidecar。
用 Evaluation Plane 提供证据。
用 Memory / Optimization Plane 沉淀经验。
用 Governance & Policy Plane 控制采纳。
用 Final Gate 和 human approval 保持最高准入约束。
```

一句话：

> Token-Efficient Agent Harness 的下一阶段不是 Stage 5，也不是 autonomous agent runtime，而是 Controlled Adaptive Orchestrator Kernel 的治理闭环建设。
