# Project Brief

Source: harness_architecture_book_v0.7.4.1-canonical §6.1

```yaml
project_brief:
  project_id: proj_2026_stage0_schema_validation
  title: "Stage 0 Harness Schema Validation Project"
  objective: "用一个最小真实项目验证 Project Board、Module Contract、Test Case Pack、Task Runtime Schema、Advisor Protocol、Batch Digest 是否够用。"
  background: >
    架构书 v0.7.4.1 定义了 Project Management Plane 和 Task Runtime 的完整 schema。
    Stage 0 的目标是不用 runtime，手动用 5 个真实任务验证这些 schema 是否能承载
    项目状态、依赖、模块边界、验收、审批和失败恢复。
  non_goals:
    - "不实现 runtime 或自动化执行引擎"
    - "不修改 src/、tests/、runtime/、.runtime/、.git/"
    - "不安装依赖或引入外部服务"
    - "不做完整 Web UI"
  constraints:
    - "只在 docs/stage0/ 下操作"
    - "所有任务手动模拟，不调用真实 API"
    - "单个模块必须能在单个 active context 内完成（preferred 80k tokens, max 200k）"
    - "所有写入必须有 Project Board 状态回写"
  success_criteria:
    - "Project Board 能承载 5 个 item 的状态流转"
    - "Project Dependency Graph 能表达 hard/soft 依赖"
    - "Module Contract 能约束任务边界（input/output/test/context budget）"
    - "Test Case Pack 能驱动验收（至少 2 个 required tests per module）"
    - "Task Runtime schema（task_spec/events/completion/handoff_pack）够用"
    - "Advisor Protocol 至少被调用 2 次"
    - "至少 1 次失败进入 Fix Loop 并完整记录"
    - "Batch Digest 能让人明确判断下一步动作"
    - "最近 3 个任务不再修改核心 schema"
  delivery_artifacts:
    - "project_brief.md（本文件）"
    - "project_board.md（5 个 item，状态流转）"
    - "project_dependency_graph.md（5 个 node，3 条 edge）"
    - "module_contracts/module_001.md, module_002.md"
    - "test_case_packs/module_001_tests.md, module_002_tests.md"
    - "5 个 task 目录的完整运行记录"
    - "batch_digest.md"
  risk_level: low
  preferred_stage: stage0
  human_owner: ""
```
