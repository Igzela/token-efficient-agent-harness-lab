export const meta = {
  name: 'batch2-dynamic-workflow-controller',
  description: 'Implement DynamicWorkflowController: observe, choose, tick, evaluate, mutate, pause/finish loop',
  phases: [
    { title: 'Research', detail: 'Explore existing code and plan controller design' },
    { title: 'Implement', detail: 'Write DynamicWorkflowController and tests' },
    { title: 'Verify', detail: 'Run tests, clippy, handoff check' },
  ],
}

phase('Research')

const researchResult = await agent(
  'Read these files and summarize the key types, methods, and patterns needed for DynamicWorkflowController:\n' +
  '1. engine/src/storage/local_product_store/workflow_runs.rs - focus on tick_with_executor_and_command, find_ready_node_locked, check_run_completion_locked, apply_dag_mutations_batch, replay_mutation_events\n' +
  '2. engine/src/workflow/dag_manager/mod.rs - focus on DAGManager, apply_mutation, nodes_ready, validate_dag\n' +
  '3. engine/src/workflow/dag_manager/types.rs - focus on DAGMutationProposal, DAGNode, DAGEdge, DAGState\n' +
  '4. engine/src/node_executor/mod.rs - focus on NodeExecutor trait, NodeExecutionInput, NodeExecutionOutput\n' +
  '5. engine/src/storage/local_product_store/workflow_runs_mutation_tests.rs - focus on test patterns and helpers\n' +
  '6. docs/NEXT_DECISION.md - Batch 2 spec\n\n' +
  'Output a structured summary with:\n' +
  '- Key types and their fields\n' +
  '- Existing methods that can be called\n' +
  '- The controller loop design: observe, choose, tick, evaluate, mutate, pause/finish\n' +
  '- What new storage methods are needed vs what exists\n' +
  '- Test patterns to follow',
  { label: 'research', phase: 'Research' }
)

log('Research complete: ' + (researchResult || '').substring(0, 200))

phase('Implement')

const controllerImpl = await agent(
  'Implement DynamicWorkflowController in engine/src/workflow/dynamic_controller.rs.\n\n' +
  'The controller owns a single-tick loop that:\n' +
  '1. Observes run state (status, nodes, edges, events) from LocalProductStore\n' +
  '2. Chooses next action based on state (tick, pause for approval, mutate graph, finish)\n' +
  '3. Ticks executor for ready nodes via existing tick_with_executor_and_command\n' +
  '4. Evaluates results and decides whether to mutate the graph\n' +
  '5. Applies mutations via apply_dag_mutations_batch\n' +
  '6. Returns a ControllerTickResult with actions taken\n\n' +
  'Key design:\n' +
  '- DynamicControllerConfig: max_ticks_per_run, max_mutations_per_run, approval_required_for_mutation, auto_fix_on_failure\n' +
  '- ControllerTickResult: actions (Vec of ControllerAction), run_status, mutations_applied, should_continue\n' +
  '- ControllerAction enum: NodeExecuted, NodeRetried, GraphMutated, ApprovalRequested, RunCompleted, RunFailed, NoAction\n' +
  '- The controller does NOT run in a loop by itself - it is called once per tick, returns whether to continue\n' +
  '  This keeps it compatible with existing scheduler tick path\n\n' +
  'File: engine/src/workflow/dynamic_controller.rs\n\n' +
  'Use these existing methods:\n' +
  '- store.get_workflow_run(run_id) - get full run state\n' +
  '- store.tick_with_executor_and_command(run_id, actor, max_retries, executor, command) - execute one node\n' +
  '- store.apply_dag_mutations_batch(run_id, dag_id, proposals, actor) - apply mutations\n' +
  '- store.replay_mutation_events(run_id) - get current graph after mutations\n' +
  '- store.append_workflow_run_event(run_id, node_id, event_type, details, actor) - record events\n\n' +
  'The controller should:\n' +
  '- On node failure with auto_fix_on_failure=true: create a fix node + dependent test node, add edges from failed node\n' +
  '- On node completed with quality check: if quality fails, create review node\n' +
  '- On all nodes done: mark run completed\n' +
  '- On mutation limit reached: pause run with approval_required status\n' +
  '- Return should_continue=true if there are still pending nodes to process\n\n' +
  'Constraints:\n' +
  '- Do NOT create a parallel scheduler or DAG kernel\n' +
  '- Do NOT enable provider execution\n' +
  '- Do NOT write to target repo\n' +
  '- Use existing NodeExecutor trait\n' +
  '- All mutations go through existing DAGMutationProposal path',
  { label: 'controller-impl', phase: 'Implement', model: 'opus' }
)

log('Controller implementation: ' + (controllerImpl || '').substring(0, 200))

const controllerTests = await agent(
  'Write tests for DynamicWorkflowController in engine/src/workflow/dynamic_controller_tests.rs.\n\n' +
  'Test patterns from workflow_runs_mutation_tests.rs:\n' +
  '- Use LocalProductStore::new(":memory:") for test store\n' +
  '- Create plans with create_workflow_plan, then create runs with create_workflow_run_from_plan\n' +
  '- Use NoopNodeExecutor for deterministic tests (from node_executor/mod.rs)\n\n' +
  'Required test cases:\n' +
  '1. test_controller_tick_executes_ready_node - basic tick executes a pending node\n' +
  '2. test_controller_tick_returns_no_action_when_no_ready_nodes - all nodes done or blocked\n' +
  '3. test_controller_completes_run_when_all_nodes_done - run transitions to completed\n' +
  '4. test_controller_fails_run_on_node_failure - node fails after max retries\n' +
  '5. test_controller_creates_fix_nodes_on_failure - auto_fix_on_failure creates fix+test nodes\n' +
  '6. test_controller_records_mutation_events - mutations are recorded as dag.mutation.* events\n' +
  '7. test_controller_respects_mutation_limit - stops mutating after max_mutations_per_run\n' +
  '8. test_controller_returns_should_continue_while_pending - returns true when work remains\n' +
  '9. test_controller_should_continue_false_when_done - returns false when run is terminal\n' +
  '10. test_controller_mutation_produces_valid_dag - graph stays valid after mutations\n\n' +
  'For each test:\n' +
  '- Create a plan with specific node structure (e.g., 2 nodes, 1 edge)\n' +
  '- Create a run from the plan\n' +
  '- Create a DynamicController with specific config\n' +
  '- Call controller.tick_once(store, run_id, executor, actor)\n' +
  '- Assert ControllerTickResult fields\n' +
  '- Verify store state via get_workflow_run\n\n' +
  'File: engine/src/workflow/dynamic_controller_tests.rs\n\n' +
  'Add mod declaration in engine/src/workflow/mod.rs:\n' +
  '  pub mod dynamic_controller;\n' +
  '  #[cfg(test)] mod dynamic_controller_tests;\n\n' +
  'Constraints:\n' +
  '- Test-first: write tests that define the expected behavior\n' +
  '- Use existing test patterns from workflow_runs_mutation_tests.rs\n' +
  '- All tests must pass cargo test\n' +
  '- Do NOT use real providers or CLI executors',
  { label: 'controller-tests', phase: 'Implement', model: 'opus' }
)

log('Tests written: ' + (controllerTests || '').substring(0, 200))

phase('Verify')

const verifyResult = await agent(
  'Run these verification commands and report results:\n\n' +
  '1. cargo test -p engine --lib workflow::dynamic_controller 2>&1 | tail -30\n' +
  '   - All new controller tests must pass\n\n' +
  '2. cargo test -p engine 2>&1 | tail -5\n' +
  '   - All 1125+ tests must pass, no regressions\n\n' +
  '3. cargo clippy -p engine -- -D warnings 2>&1 | tail -20\n' +
  '   - No clippy warnings\n\n' +
  '4. uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n' +
  '   - Handoff check must pass\n\n' +
  '5. If any test fails, fix the issue and re-run.\n' +
  '6. If clippy warns, fix the warning.\n\n' +
  'Report: test count, pass/fail, clippy status, handoff status.',
  { label: 'verify', phase: 'Verify' }
)

log('Verification: ' + (verifyResult || '').substring(0, 300))

return {
  batch: 'Batch 2: DynamicWorkflowController',
  controller: controllerImpl ? 'implemented' : 'failed',
  tests: controllerTests ? 'written' : 'failed',
  verification: verifyResult || 'unknown',
}
