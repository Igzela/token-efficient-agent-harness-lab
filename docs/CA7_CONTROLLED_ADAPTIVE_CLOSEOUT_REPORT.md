# CA-7 Controlled Adaptive Closeout Report

版本：v1.0
状态：Controlled Adaptive Orchestrator Kernel minimum threshold reached
定位：基于 v1.3.1 架构书 CA gates 的正式完成报告；不替代现有文档。
生成时间：2026-05-19
基线 commit：`6c75220`
测试命令：`PYTHONPATH=src python3 -m unittest discover -s tests`
测试结果：751 tests, OK

---

## 1. Executive Summary

Token-Efficient Agent Harness Lab 已达到 Controlled Adaptive Orchestrator Kernel 的最低门槛（CA-0 至 CA-7 全部通过）。系统现在具备：

- 确定性 Orchestrator Core（Stage 0-4）
- 完整的 Evaluation Plane（real-world eval、harness change eval、fixture expansion、user-style mutation eval、usage ledger / cost-of-pass）
- Tool/Error Taxonomy（10 个 error domains，unknown_error 强制 triage）
- Context Pack v2（五层结构与 v1.2 canonical schema 共存）
- Policy Candidate Lifecycle（manifest、evidence pack、approval record、rollback plan、policy registry）
- Governance Approval Path（五 gate 强制执行，no activation without admitted evidence + rollback plan + human approval）
- Model Profiles and Shadow Routing（diagnostic-only，active routing 需 human approval）

系统不是 Adaptive Cognitive Kernel。当前声明为：

> **Controlled Adaptive Orchestrator Kernel minimum threshold reached.**

## 2. Status Declaration

根据 v1.3.1 架构书 Section 14 的分类规则：

- CA-0 至 CA-2：adaptive-ready
- CA-3 至 CA-5：evaluation-controlled adaptive preparation
- CA-6 至 CA-7：Controlled Adaptive Orchestrator Kernel

**当前状态：CA-7 已通过，Controlled Adaptive Orchestrator Kernel 最低门槛达成。**

这不是 Adaptive Cognitive Kernel，也不意味着系统可以自主部署策略。所有 policy adoption 仍需 admitted evidence + rollback plan + human approval。

## 3. CA Gate Evidence Matrix

### CA-0: Orchestrator Kernel sealed

| 维度 | 证据 |
|------|------|
| Stage 0-4 complete | `docs/PROJECT_CLOSEOUT_REPORT.md` — Stage 0-4 封版报告 |
| CI passing | 751 tests, OK |
| Runtime boundary documented | `docs/architecture/harness_architecture_book_v1.2_authority_kernel_addendum.md` |
| Architecture book | `docs/architecture/harness_architecture_book_v1.3_controlled_adaptive_orchestrator.md` |

**状态：通过**

### CA-1: Evaluation suite stable

| 维度 | 证据 |
|------|------|
| Deterministic fixtures | `tests/fixtures/stage0_events_with_line17_issue.jsonl`、`tests/fixtures/stage0_events_sanitized.jsonl` |
| Real-world read-only fixtures | `tests/fixtures/real_world_eval/` (7 project fixtures) |
| Harness change evaluation | `tests/test_harness_change_eval.py`、`docs/harness_change_evaluation_plan.md` |
| Scoring engine | `src/harness_core/scoring.py`、`tests/test_scoring.py` |
| Quality gate | `src/harness_core/quality_gate.py`、`tests/test_quality_gate.py` |
| Baseline manager | `src/harness_core/baseline.py`、`tests/test_baseline.py` |
| Trajectory monitor | `src/harness_core/trajectory.py`、`tests/test_trajectory.py` |
| Quality digest | `src/harness_core/quality_digest.py`、`tests/test_quality_digest.py` |

**状态：通过**

### CA-2: Tool/Error Taxonomy operational

| 维度 | 证据 |
|------|------|
| Error domains defined | `docs/tool_error_taxonomy.md` — 10 个 error domains |
| Error taxonomy module | `src/harness_core/error_taxonomy.py` |
| Error taxonomy tests | `tests/test_error_taxonomy.py` |
| Error fixtures | `tests/fixtures/tool_error_cases/` (10 fixtures) |
| unknown_error requires triage | `error_taxonomy.py: requires_human_triage = True for unknown_error` |
| Context error | `tests/fixtures/tool_error_cases/context_error.json` |
| Tool contract error | `tests/fixtures/tool_error_cases/tool_contract_error.json` |
| Environment error | `tests/fixtures/tool_error_cases/environment_error.json` |
| Model judgment error | `tests/fixtures/tool_error_cases/model_judgment_error.json` |
| Evaluation error | `tests/fixtures/tool_error_cases/evaluation_error.json` |
| Harness bug | `tests/fixtures/tool_error_cases/harness_bug.json` |
| User abort | `tests/fixtures/tool_error_cases/user_abort.json` |
| Provider error | `tests/fixtures/tool_error_cases/provider_error.json` |
| Timeout | `tests/fixtures/tool_error_cases/timeout.json` |

**状态：通过**

### CA-3: User-Style Mutation Eval stable

| 维度 | 证据 |
|------|------|
| formal_issue variants | `tests/fixtures/user_style_mutation_eval/*_formal_issue.json` (5 fixtures) |
| user_style_chat_request variants | `tests/fixtures/user_style_mutation_eval/*_chat_request.json` (4 fixtures) |
| terse_ticket variants | `tests/fixtures/user_style_mutation_eval/*_terse_ticket.json` (5 fixtures) |
| Eval module | `src/harness_core/user_style_mutation.py` |
| Eval tests | `tests/test_user_style_mutation_eval.py` |
| Admission logic | `user_style_mutation.py: validate_user_style_eval_result()` |
| Fixture metadata | `docs/user_style_mutation_eval.md` |

**状态：通过**

### CA-4: Context Pack v2 schema and tests ready

| 维度 | 证据 |
|------|------|
| v1.2 canonical schemas intact | `src/harness_core/context_pack.py` — advisor_context_pack_v2、model_context_pack_v2、context_retrieval_request、context_retrieval_result |
| Five-layer structure | `docs/context_pack_v2.md` — invariants、task_pack、dynamic_refs、memory_digest、recent_evidence |
| Context pack tests | `tests/test_context_pack.py` |
| Context pack fixtures | `tests/fixtures/context_pack_v2/` (14 fixtures) |
| Memory boundary enforced | `context_pack.py: memory_digest requires source, expiry, conflict handling` |
| Budget/prune policy | `context_pack.py: validate_context_pack_budget()` |

**状态：通过**

### CA-5: Usage Ledger and Cost-of-Pass available

| 维度 | 证据 |
|------|------|
| Usage ledger schema | `src/harness_core/usage_ledger.py` — usage_ledger.v1 |
| Usage ledger tests | `tests/test_usage_ledger.py` |
| Usage ledger fixtures | `tests/fixtures/usage_ledger/` (15 fixtures) |
| cost_of_pass_group defined | `docs/usage_ledger_cost_of_pass.md` |
| Token/cost/retry/tool-call tracking | `usage_ledger.py: input_tokens、output_tokens、cached_tokens、request_count、tool_call_count、retry_count、estimated_cost` |
| Group comparison rules | `usage_ledger.py: compare_cost_of_pass_groups()` |

**状态：通过**

### CA-6: Policy Candidate Lifecycle implemented

| 维度 | 证据 |
|------|------|
| policy_candidate_manifest | `src/harness_core/policy_candidate.py: validate_policy_candidate_manifest()` |
| candidate_evidence_pack | `src/harness_core/policy_candidate.py: validate_candidate_evidence_pack()` |
| approval_record | `src/harness_core/policy_candidate.py: validate_approval_record()` |
| rollback_plan | `src/harness_core/policy_candidate.py: validate_rollback_plan()` |
| policy_registry_entry | `src/harness_core/policy_candidate.py: validate_policy_registry_entry()` |
| Lifecycle tests | `tests/test_policy_candidate.py` |
| Lifecycle fixtures | `tests/fixtures/policy_candidate/` (11 fixtures) |
| Lifecycle docs | `docs/policy_candidate_lifecycle.md` |
| Adoption rules | `policy_candidate.py: can_activate_policy()、should_reject_diagnostic_only_candidate()` |

**状态：通过**

### CA-7: Governance approval path enforced

| 维度 | 证据 |
|------|------|
| governance_decision schema | `src/harness_core/governance.py: validate_governance_decision()` |
| evidence_gate | `governance.py: evaluate_evidence_gate()` — admitted_evidence_refs non-empty |
| approval_gate | `governance.py: evaluate_approval_gate()` — approval_record.decision = approved |
| rollback_gate | `governance.py: evaluate_rollback_gate()` — rollback_plan.status = approved |
| scope_gate | `governance.py: evaluate_scope_gate()` — no user project file paths |
| unknown_error_gate | `governance.py: evaluate_unknown_error_gate()` — no unknown_error or has human_review_refs |
| decide_policy_activation | `governance.py: decide_policy_activation()` — all gates must pass |
| Governance tests | `tests/test_governance.py` |
| Governance fixtures | `tests/fixtures/governance/` (12 fixtures) |
| Governance docs | `docs/governance_approval_path.md` |
| Cross-reference: policy candidate lifecycle | `tests/fixtures/governance/cross_reference_policy_candidate_lifecycle.json` |
| Cross-reference: shadow routing | `tests/fixtures/governance/cross_reference_shadow_routing.json` |
| Cross-reference: unknown error | `tests/fixtures/governance/cross_reference_unknown_error.json` |

**状态：通过**

## 4. Track Evidence Summary

| Track | Source Module | Test File | Fixture Dir | Test Count | Boundary Confirmed |
|-------|---------------|-----------|-------------|------------|-------------------|
| Event Store / Kernel Core | `event_store.py`, `kernel.py`, `event_schema.py` | `test_event_store.py`, `test_kernel.py` | `tests/fixtures/stage0_*.jsonl` | ~60 | No real model, no real agents |
| Projection / Board / Queue | `projection_store.py`, `project_board.py`, `task_queue.py` | `test_projection_store.py`, `test_project_board.py`, `test_task_queue.py` | — | ~50 | Deterministic state transitions |
| Validators / Digest | `validators.py`, `digest.py` | `test_validators.py`, `test_digest.py` | — | ~40 | Schema-only validation |
| CLI / BatchRunner | `cli.py`, `batch_runner.py` | `test_cli.py`, `test_batch_runner.py` | — | ~30 | Local execution only |
| Task Records / Final Gate | `task_records.py`, `final_gate.py` | `test_task_records.py`, `test_final_gate.py` | — | ~35 | Human approval at final gate |
| Orchestrator | `orchestrator.py` | `test_orchestrator.py` | — | ~15 | Deterministic coordination |
| Scoring / Quality Gates | `scoring.py`, `quality_gate.py`, `artifact_gate.py`, `quality_digest.py` | `test_scoring.py`, `test_quality_gate.py`, `test_artifact_gate.py`, `test_quality_digest.py` | — | ~55 | No runtime modification |
| Evaluation / Baseline / Trajectory | `evaluation.py`, `baseline.py`, `trajectory.py` | `test_evaluation.py`, `test_baseline.py`, `test_trajectory.py` | — | ~45 | Read-only evaluation |
| Advisor / Model Gateway / Routing / Sampling / Skills | `advisor.py`, `model_gateway.py`, `routing.py`, `sampling.py`, `skills.py` | `test_advisor.py`, `test_model_gateway.py`, `test_routing.py`, `test_sampling.py`, `test_skills.py` | — | ~60 | Stubs only, no real model calls |
| DAG / Sandbox / Concurrency / Supervisor / Checkpoint | `dag_manager.py`, `dag_mutations.py`, `sandbox.py`, `concurrency.py`, `supervisor.py`, `checkpoint.py` | `test_dag_manager.py`, `test_dag_mutations.py`, `test_sandbox.py`, `test_concurrency.py`, `test_supervisor.py`, `test_checkpoint.py` | — | ~65 | No real process isolation |
| Artifact Lifecycle / Health / Dashboard | `artifact_lifecycle.py`, `health.py`, `dashboard_model.py` | `test_artifact_lifecycle.py`, `test_health.py`, `test_dashboard_model.py` | — | ~30 | State machine only |
| Real-World Eval / Harness Change Eval | — | `test_real_world_eval.py`, `test_harness_change_eval.py` | `tests/fixtures/real_world_eval/` | ~20 | Read-only, no real execution |
| Error Taxonomy | `error_taxonomy.py` | `test_error_taxonomy.py` | `tests/fixtures/tool_error_cases/` | ~25 | Classification only, no retry for unknown_error |
| User-Style Mutation Eval | `user_style_mutation.py` | `test_user_style_mutation_eval.py` | `tests/fixtures/user_style_mutation_eval/` | ~20 | Three variant types, admission logic |
| Context Pack v2 | `context_pack.py` | `test_context_pack.py` | `tests/fixtures/context_pack_v2/` | ~25 | Budget enforcement, memory boundary |
| Usage Ledger / Cost-of-Pass | `usage_ledger.py` | `test_usage_ledger.py` | `tests/fixtures/usage_ledger/` | ~20 | Cost comparison within groups only |
| Model Profiles / Shadow Routing | `model_profiles.py` | `test_model_profiles.py` | `tests/fixtures/model_profiles/` | ~20 | Shadow = diagnostic only, active routing = false |
| Policy Candidate Lifecycle | `policy_candidate.py` | `test_policy_candidate.py` | `tests/fixtures/policy_candidate/` | ~25 | No adoption without admitted evidence |
| Governance Approval Path | `governance.py` | `test_governance.py` | `tests/fixtures/governance/` | ~25 | Five gates enforced, governance never executes deployment |

**Total: 751 tests, 42 source modules, 43 test files, 60+ fixture directories.**

## 5. Boundary / Out-of-Scope Confirmation

以下能力在 CA-7 达成后仍然 **不在范围内**：

### Runtime 自治

- 不允许 Orchestrator 自己改目标
- 不允许 Orchestrator 写长期记忆
- 不允许 runtime 自动部署 routing policy
- 不允许 LLM critique 直接通过 Final Gate
- 不允许 shadow routing 直接变 active routing
- 不允许 skill extraction 自动改 prompt
- 不允许 policy candidate 绕过 offline eval
- 不允许 unknown_error 驱动 adaptive candidate
- 不允许 diagnostic evidence 单独决定 policy adoption

### 真实执行

- 不允许真实模型执行任务
- 不允许真实 sandbox 执行
- 不允许真实并发 worker
- 不允许真实多 agent runtime
- 不允许真实 provider failover

### 项目管理

- 不允许自动 PR / merge
- 不允许自动创建 Stage 5 来规避 post-closeout track gate
- 不允许创建新 stage 来绕过 CA gate

### 基线完整性

- `docs/stage0/events.jsonl` 保持不变
- 现有架构文档（v0.7.4.1、v1.2、v1.2-addendum、v1.3.1）不被修改
- 现有 source modules 不被修改
- 现有 test files 不被修改

## 6. Governance Meaning of CA-7

CA-7 的通过意味着：

1. **Governance & Policy Plane 完整建立**：policy candidate lifecycle（manifest → evidence pack → approval → rollback → registry）和 governance approval path（五 gate 强制执行）都已实现并测试。

2. **No activation without governance**：任何 policy adoption 必须通过：
   - admitted evidence gate（至少一个 admitted_evidence_refs）
   - approval gate（approval_record.decision = approved）
   - rollback gate（rollback_plan.status = approved，steps non-empty）
   - scope gate（impacted_refs 中无 user project file paths）
   - unknown_error gate（无 unknown_error evidence，或有 human_review_refs）

3. **Governance never executes deployment**：Governance 只做决策，不执行部署。部署由 human/tool 通过 policy registry update 执行。

4. **Seal 当前基线**：CA-7 通过后，当前 harness policy baseline 被封存。未来改动必须通过 policy candidate lifecycle。

## 7. Seal Semantics

CA-7 达成后：

- **当前基线封存**：所有已实现的 harness policy、config、schema、profile、skill、eval gate 构成当前 sealed baseline。
- **封存范围**：source modules、test suite、fixtures、docs、architecture books 均在封存范围内。
- **未来调整路径**：可通过明确的版本化补丁调整 sealed baseline，但必须：
  - 创建 policy_candidate_manifest
  - 提供 candidate_evidence_pack
  - 通过 governance approval path（五 gate）
  - 获得 human approval
  - 提供 rollback_plan
  - 更新 policy_registry_entry
- **不可绕过**：任何试图绕过 governance approval path 的改动都是对 sealed baseline 的违规。
- **版本化**：未来补丁应以版本号标记（如 baseline v1.0 → v1.1），并在 policy_registry_entry 中记录。

## 8. Remaining Future Tracks

以下 Track 在 CA-7 达成后可被选择性推进，但 **不作为默认下一步**，且需要单独审批：

### Track 8: Advisor-Only Real Model Test（CA-8）

- 定位：controlled real-model advisory mode
- 前置条件：必须满足 v1.3.1 Section 16 全部条件（Tool/Error Taxonomy、User-Style Mutation Eval、Context Pack v2、Usage Ledger、Model Harness Profile、Evaluation Admission Contract、Policy Candidate Lifecycle、Provider credentials 不进 repo、预算上限明确、人工 approval 明确）
- 限制：模型只能做 Advisor Preflight、Advisor Correction、Advisor Risk Scan、Offline Critique、Candidate Ranking
- 禁止：文件修改、shell command、sandbox execution、PR 创建、自动 merge、自动 policy adoption、自动 prompt mutation
- 审批要求：需要单独的人工审批

### 其他 Optional Future Tracks

- Productionization：durable storage, service boundaries, operational configuration
- Real sandbox execution: OS/process/container isolation, cleanup, and security model
- UI/dashboard: presentation layer using the Stage 4 dashboard data model
- Packaging: installable distribution, versioning, release artifacts
- Benchmarks: performance baselines and regression tracking
- Security review: threat model, secret handling, supply-chain checks, sandbox policy

每个 future track 需要单独审批和新的实施计划。

## 9. Recommendation

1. **接受当前状态**：Controlled Adaptive Orchestrator Kernel minimum threshold reached。不声称 Adaptive Cognitive Kernel。

2. **不自动推进 CA-8**：CA-8（Advisor-Only Real Model Test）作为 Optional Future Track，需要单独审批。不要将其作为默认下一步。

3. **封存当前基线**：所有当前 harness policy、config、schema、profile、skill、eval gate 被视为 sealed baseline。

4. **维护治理闭环**：任何未来改动必须通过 policy candidate lifecycle 和 governance approval path。

5. **保留现有文档**：不修改现有架构文档。本报告作为独立文件存在。

6. **测试命令保持不变**：
   ```bash
   PYTHONPATH=src python3 -m unittest discover -s tests
   ```
   当前结果：751 tests, OK。

---

*本报告基于 v1.3.1 架构书 CA gates 的正式完成报告。不替代现有文档，不修改 runtime 行为，不修改 `docs/stage0/events.jsonl`。*
