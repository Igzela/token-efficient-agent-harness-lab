export const meta = {
  name: 'batch7-e2e-trial',
  description: 'Dynamic Workflow E2E Trial: broad task, plan, execute, test fails, graph mutates, rerun, review/approval, export',
  phases: [
    { title: 'Research', detail: 'Understand all prior batch components' },
    { title: 'Implement', detail: 'Write E2E trial test and integration' },
    { title: 'Verify', detail: 'Run tests, clippy, handoff check' },
    { title: 'Ship', detail: 'Commit, push, CI check, docs update, memory update' },
  ],
}

phase('Research')

const researchResult = await agent(
  'Read these files to understand the full Dynamic Workflow stack for Batch 7 E2E Trial:\n' +
  '1. engine/src/workflow/dynamic_controller.rs - DynamicWorkflowController::tick\n' +
  '2. engine/src/workflow/dynamic_decomposer.rs - Decomposer trait, RuleBasedDecomposer\n' +
  '3. engine/src/workflow/agent_profiles.rs - AgentProfile, default profiles\n' +
  '4. engine/src/workflow/tool_registry.rs - ToolRegistry, hooks, allowlists\n' +
  '5. engine/src/routing/feedback_store.rs - FeedbackStore, insert_scheduler_feedback\n' +
  '6. engine/src/storage/local_product_store/workflow_runs.rs - tick_with_executor_and_command\n' +
  '7. engine/src/node_executor/mod.rs - NodeExecutor, NoopNodeExecutor, FailNodeExecutor\n' +
  '8. engine/src/storage/local_product_store/workflow_runs_mutation_tests.rs - test patterns\n\n' +
  'Output:\n' +
  '- Full API surface of each component\n' +
  '- How to wire them together for E2E\n' +
  '- What the E2E test should assert: graph mutation events, patch contents, test logs, integrity, approval, cleanup\n' +
  '- What approval workflow looks like end-to-end',
  { label: 'research', phase: 'Research' }
)

log('Research: ' + (researchResult || '').substring(0, 200))

phase('Implement')

const e2eImpl = await agent(
  'Implement Batch 7: Dynamic Workflow E2E Trial in engine/src/workflow/dynamic_workflow_e2e_tests.rs.\n\n' +
  'This is the culminating test that proves the full dynamic workflow stack works end-to-end.\n\n' +
  'Test scenario:\n' +
  '1. Create a workflow plan with 2 initial nodes: analyze -> execute\n' +
  '2. Create a workflow run from the plan\n' +
  '3. Set up DynamicWorkflowController with auto_fix_on_failure=true\n' +
  '4. Set up RuleBasedDecomposer as the decomposer\n' +
  '5. Set up agent profiles (implementer, tester, reviewer)\n' +
  '6. Register tool capabilities (read, write, bash)\n' +
  '7. Add a PreExecution hook that logs tool usage\n\n' +
  'E2E flow:\n' +
  'Step A: tick controller -> analyze node executes (NoopNodeExecutor, succeeds)\n' +
  '  Assert: run status=running, analyze node completed, feedback recorded\n\n' +
  'Step B: tick controller -> execute node executes (FailNodeExecutor, fails)\n' +
  '  Assert: execute node failed, auto-fix triggered\n\n' +
  'Step C: tick controller -> fix node executes (NoopNodeExecutor, succeeds)\n' +
  '  Assert: fix node completed, test node is now pending\n\n' +
  'Step D: tick controller -> test node executes (NoopNodeExecutor, succeeds)\n' +
  '  Assert: test node completed, run status=completed\n\n' +
  'Step E: verify full event trail\n' +
  '  Assert: workflow_run_events contains:\n' +
  '    - workflow_run.created\n' +
  '    - workflow_run.tick_started\n' +
  '    - node.leased (multiple)\n' +
  '    - node.completed (analyze, fix, test)\n' +
  '    - node.failed (execute)\n' +
  '    - dag.mutation.node_added (fix, test nodes)\n' +
  '    - dag.mutation.edge_added (edges to fix, test)\n' +
  '    - workflow_run.completed\n\n' +
  'Step F: verify feedback records\n' +
  '  Assert: scheduler_feedback has entries for each tick\n' +
  '  Assert: suggest_executor_type returns based on history\n\n' +
  'Step G: verify graph integrity\n' +
  '  Assert: replay_mutation_events produces valid DAG\n' +
  '  Assert: all nodes have profile_id set\n' +
  '  Assert: tool allowlist was checked\n\n' +
  'Step H: verify export/import round-trip\n' +
  '  Assert: export_workflow_runs includes all events\n' +
  '  Assert: re-import preserves graph state\n\n' +
  'Test naming: test_e2e_dynamic_workflow_full_cycle\n\n' +
  'File: engine/src/workflow/dynamic_workflow_e2e_tests.rs\n\n' +
  'Add mod in engine/src/workflow/mod.rs:\n' +
  '  #[cfg(test)] mod dynamic_workflow_e2e_tests;\n\n' +
  'Constraints:\n' +
  '- Do NOT create parallel scheduler/runtime/DAG kernel\n' +
  '- Do NOT enable provider execution\n' +
  '- Do NOT write to target repo\n' +
  '- Use only NoopNodeExecutor and FailNodeExecutor\n' +
  '- All assertions must be explicit and detailed\n' +
  '- Test must pass cargo test',
  { label: 'e2e-impl', phase: 'Implement', model: 'opus' }
)

log('E2E impl: ' + (e2eImpl || '').substring(0, 200))

phase('Verify')

const verifyResult = await agent(
  'Run these verification commands and report results:\n\n' +
  '1. cargo test -p engine --lib workflow::dynamic_workflow_e2e 2>&1 | tail -30\n' +
  '2. cargo test -p engine 2>&1 | tail -5\n' +
  '3. cargo clippy -p engine -- -D warnings 2>&1 | tail -20\n' +
  '4. uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n\n' +
  'If any test fails, fix and re-run. Report: test count, pass/fail, clippy, handoff.',
  { label: 'verify', phase: 'Verify' }
)

log('Verify: ' + (verifyResult || '').substring(0, 300))

phase('Ship')

const shipResult = await agent(
  'Complete the shipping process for Batch 7 (FINAL BATCH):\n\n' +
  '1. Get current test count:\n' +
  '   cargo test -p engine 2>&1 | grep -E "test result:|passed"\n\n' +
  '2. Update docs/CURRENT_STATUS.md:\n' +
  '   - Update test count in Repository State section\n' +
  '   - Update Dynamic Workflow Batch line: ALL 7 BATCHES COMPLETE\n' +
  '   - Update Dynamicity assessment: High\n' +
  '   - Update minimum acceptance target as achieved\n\n' +
  '3. Update docs/NEXT_DECISION.md:\n' +
  '   - Mark Batch 7 as DONE in the batches table\n' +
  '   - Update "Current status" to: ALL BATCHES COMPLETE\n' +
  '   - Mark Dynamic Workflow Direction as complete\n\n' +
  '4. Update docs/MODULE_MAP.md:\n' +
  '   - Update all Dynamic Workflow rows to DONE\n\n' +
  '5. Update CLAUDE.md test count\n\n' +
  '6. Update AGENTS.md test count\n\n' +
  '7. Commit all changes:\n' +
  '   git add -A\n' +
  '   git commit -m "Batch 7: Dynamic Workflow E2E Trial — full cycle prove-out, ALL BATCHES COMPLETE (11XX tests)"\n\n' +
  '8. Push:\n' +
  '   git push\n\n' +
  '9. Check CI status (wait up to 5 minutes, check every 30s):\n' +
  '   gh run list --branch feat/dashboard-ux-polish --limit 1 --json status,conclusion\n' +
  '   If conclusion != "success", get the log and fix the issue:\n' +
  '   gh run view <run_id> --log-failed\n' +
  '   Fix the failing test/lint, commit, push again, re-check.\n\n' +
  '10. Write memory file at /home/igzela/.claude/projects/-home-igzela-Projects-token-efficient-agent-harness-lab/memory/project_dynamic_workflow_all_batches_closeout.md\n\n' +
  '11. Update MEMORY.md index with new entry.\n\n' +
  'Report final status: commit hash, test count, CI status.',
  { label: 'ship', phase: 'Ship' }
)

log('Ship: ' + (shipResult || '').substring(0, 300))

return {
  batch: 'Batch 7: Dynamic Workflow E2E Trial (FINAL)',
  implementation: e2eImpl ? 'done' : 'failed',
  verification: verifyResult || 'unknown',
  shipping: shipResult || 'unknown',
}
