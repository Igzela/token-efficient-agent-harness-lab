export const meta = {
  name: 'batch4-dynamic-decomposition',
  description: 'Replace fixed decomposition with planner interface supporting observation/test-failure/quality-failure/user-goal driven proposals',
  phases: [
    { title: 'Research', detail: 'Explore existing decomposition and planner code' },
    { title: 'Implement', detail: 'Write DynamicDecomposer and tests' },
    { title: 'Verify', detail: 'Run tests, clippy, handoff check' },
    { title: 'Ship', detail: 'Commit, push, CI check, docs update, memory update' },
  ],
}

phase('Research')

const researchResult = await agent(
  'Read these files and summarize what exists for Batch 4 Dynamic Decomposition:\n' +
  '1. engine/src/orchestration/task_decomposer.rs - current fixed simple/medium/complex decomposition\n' +
  '2. engine/src/read_only_planner.rs - ReadOnlyPlanner, how it uses TaskDecomposer\n' +
  '3. engine/src/task_analyzer/mod.rs - TaskAnalysis struct, complexity_score, risk_flags\n' +
  '4. engine/src/workflow/dynamic_controller.rs - how controller currently handles mutations (auto-fix, quality review)\n' +
  '5. engine/src/workflow/dag_manager/types.rs - DAGMutationProposal\n\n' +
  'Output:\n' +
  '- Current decomposition logic (simple/medium/complex thresholds)\n' +
  '- TaskAnalysis fields available for decomposition decisions\n' +
  '- What DynamicDecomposer trait/interface should look like\n' +
  '- How DynamicWorkflowController should call DynamicDecomposer\n' +
  '- What proposals should be generated from observations, test failures, quality failures, user goals',
  { label: 'research', phase: 'Research' }
)

log('Research: ' + (researchResult || '').substring(0, 200))

phase('Implement')

const decomposerImpl = await agent(
  'Implement Batch 4: Dynamic Decomposition in engine/src/workflow/dynamic_decomposer.rs.\n\n' +
  'Current state: TaskDecomposer does fixed simple/medium/complex graph generation from TaskAnalysis.\n' +
  'Goal: Replace with an extensible Decomposer trait that supports observation-driven, test-failure-driven, quality-failure-driven, and user-goal-driven node proposals.\n\n' +
  'New file: engine/src/workflow/dynamic_decomposer.rs\n\n' +
  'Key types:\n' +
  '- DecompositionTrigger enum: Observation(String), TestFailure { node_id, error }, QualityFailure { node_id, reason }, UserGoal(String), InitialPlan(TaskAnalysis)\n' +
  '- NodeProposal: node_id, node_type, task_type, depends_on (Vec), reason, priority (u8)\n' +
  '- DecompositionResult: proposals (Vec of NodeProposal), strategy (String), metadata (Value)\n' +
  '- trait Decomposer: fn decompose(&self, trigger: DecompositionTrigger, context: &DecompositionContext) -> DecompositionResult\n' +
  '- DecompositionContext: run_id, existing_nodes (Vec), existing_edges (Vec), feedback_stats (Option), max_nodes (usize)\n\n' +
  'RuleBasedDecomposer (default implementation):\n' +
  '- InitialPlan: same logic as current TaskDecomposer (simple/medium/complex)\n' +
  '- TestFailure: propose fix node + test node depending on failed node\n' +
  '- QualityFailure: propose review node depending on source node\n' +
  '- Observation: if feedback shows high failure rate for executor_type, propose alternative executor node\n' +
  '- UserGoal: parse goal text, propose analyze + execute + verify nodes\n\n' +
  'Integration with DynamicWorkflowController:\n' +
  '- Add decomposer: Option<Box<dyn Decomposer>> to DynamicControllerConfig\n' +
  '- In tick(), after observing run state, call decomposer.decompose() with appropriate trigger\n' +
  '- Convert NodeProposal to DAGMutationProposal and apply via store.apply_dag_mutations_batch\n' +
  '- This replaces the hardcoded auto-fix and quality-review logic in current controller\n\n' +
  'New file: engine/src/workflow/dynamic_decomposer_tests.rs\n' +
  'Required tests:\n' +
  '1. test_initial_plan_simple_decomposition\n' +
  '2. test_initial_plan_complex_decomposition\n' +
  '3. test_test_failure_triggers_fix_proposals\n' +
  '4. test_quality_failure_triggers_review_proposals\n' +
  '5. test_observation_triggers_alternative_executor\n' +
  '6. test_user_goal_triggers_analyze_execute_verify\n' +
  '7. test_max_nodes_limits_proposals\n' +
  '8. test_existing_nodes_not_duplicated\n' +
  '9. test_decomposer_integrates_with_controller\n' +
  '10. test_empty_context_returns_empty_proposals\n\n' +
  'Add mod in engine/src/workflow/mod.rs:\n' +
  '  pub mod dynamic_decomposer;\n' +
  '  #[cfg(test)] mod dynamic_decomposer_tests;\n\n' +
  'Update DynamicWorkflowController to use Decomposer trait:\n' +
  '- Replace hardcoded auto_fix logic with decomposer.decompose(DecompositionTrigger::TestFailure(...))\n' +
  '- Replace hardcoded quality_review logic with decomposer.decompose(DecompositionTrigger::QualityFailure(...))\n' +
  '- Keep backward compatibility: if no decomposer set, use RuleBasedDecomposer by default\n\n' +
  'Constraints:\n' +
  '- Do NOT create a parallel scheduler or DAG kernel\n' +
  '- Do NOT enable provider execution\n' +
  '- Extensible interface: future batches can add CLI/provider-backed decomposers\n' +
  '- All tests use in-memory store with NoopNodeExecutor',
  { label: 'decomposer-impl', phase: 'Implement', model: 'opus' }
)

log('Decomposer impl: ' + (decomposerImpl || '').substring(0, 200))

phase('Verify')

const verifyResult = await agent(
  'Run these verification commands and report results:\n\n' +
  '1. cargo test -p engine --lib workflow::dynamic_decomposer 2>&1 | tail -30\n' +
  '2. cargo test -p engine --lib workflow::dynamic_controller 2>&1 | tail -10\n' +
  '3. cargo test -p engine 2>&1 | tail -5\n' +
  '4. cargo clippy -p engine -- -D warnings 2>&1 | tail -20\n' +
  '5. uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n\n' +
  'If any test fails, fix and re-run. Report: test count, pass/fail, clippy, handoff.',
  { label: 'verify', phase: 'Verify' }
)

log('Verify: ' + (verifyResult || '').substring(0, 300))

phase('Ship')

const shipResult = await agent(
  'Complete the shipping process for Batch 4:\n\n' +
  '1. Get current test count:\n' +
  '   cargo test -p engine 2>&1 | grep -E "test result:|passed"\n\n' +
  '2. Update docs/CURRENT_STATUS.md:\n' +
  '   - Update test count in Repository State section\n' +
  '   - Update Dynamic Workflow Batch line to include Batch 4\n' +
  '   - Update Dynamicity assessment row\n\n' +
  '3. Update docs/NEXT_DECISION.md:\n' +
  '   - Mark Batch 4 as DONE in the batches table\n' +
  '   - Update "Current status" line\n' +
  '   - Mark Batch 5 as next\n\n' +
  '4. Update docs/MODULE_MAP.md:\n' +
  '   - Update Dynamic Decomposition row to DONE\n\n' +
  '5. Update CLAUDE.md test count\n\n' +
  '6. Update AGENTS.md test count\n\n' +
  '7. Commit all changes:\n' +
  '   git add -A\n' +
  '   git commit -m "Batch 4: Dynamic Decomposition — Decomposer trait, observation/test/quality/goal driven proposals (11XX tests)"\n\n' +
  '8. Push:\n' +
  '   git push\n\n' +
  '9. Check CI status (wait up to 5 minutes, check every 30s):\n' +
  '   gh run list --branch feat/dashboard-ux-polish --limit 1 --json status,conclusion\n' +
  '   If conclusion != "success", get the log and fix the issue:\n' +
  '   gh run view <run_id> --log-failed\n' +
  '   Fix the failing test/lint, commit, push again, re-check.\n\n' +
  '10. Write memory file at /home/igzela/.claude/projects/-home-igzela-Projects-token-efficient-agent-harness-lab/memory/project_dynamic_workflow_batch4_closeout.md:\n' +
  '    ---\n' +
  '    name: project_dynamic_workflow_batch4_closeout\n' +
  '    description: Batch 4 Dynamic Decomposition closeout\n' +
  '    metadata:\n' +
  '      type: project\n' +
  '    ---\n' +
  '    [summary of what was implemented, test count, next batch]\n\n' +
  '11. Update MEMORY.md index with new entry.\n\n' +
  'Report final status: commit hash, test count, CI status.',
  { label: 'ship', phase: 'Ship' }
)

log('Ship: ' + (shipResult || '').substring(0, 300))

return {
  batch: 'Batch 4: Dynamic Decomposition',
  implementation: decomposerImpl ? 'done' : 'failed',
  verification: verifyResult || 'unknown',
  shipping: shipResult || 'unknown',
}
