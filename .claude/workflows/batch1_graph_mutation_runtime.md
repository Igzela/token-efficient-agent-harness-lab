export const meta = {
  name: 'batch1-graph-mutation-runtime',
  description: 'Batch 1: Persisted Graph Mutation Runtime for dynamic workflows',
  phases: [
    { title: 'Implement', detail: 'Write mutation storage operations in workflow_runs.rs' },
    { title: 'Test', detail: 'Write comprehensive tests for mutation storage' },
    { title: 'Verify', detail: 'Run cargo test, clippy, and handoff check' },
  ],
}

phase('Implement')

const task_2a = await agent(
  'You are implementing Batch 1 of Dynamic Workflow support.\n\n' +
  'TASK: Add 8 new mutation storage methods to LocalProductStore in:\n' +
  '/home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/workflow_runs.rs\n\n' +
  'STEP 1: Read the FULL file first.\n\n' +
  'STEP 2: Add these methods to the `impl LocalProductStore` block (add them after the existing methods, before the closing brace of the impl block that contains `update_workflow_run_status_with_event`).\n\n' +
  'METHOD 1: insert_workflow_node(run_id, node: &Value, actor, reason) -> Result<Value, String>\n' +
  '  - Checks run exists, checks for duplicate node_id, inserts node via insert_workflow_run_node_locked,\n' +
  '  - Records dag.mutation.node_added event, returns json!({ action: "node_inserted", node_id, run_id, metadata_only: true })\n\n' +
  'METHOD 2: remove_workflow_node(run_id, node_id, actor, reason) -> Result<Value, String>\n' +
  '  - Captures connected edges, deletes them, deletes the node, records dag.mutation.node_removed event\n' +
  '  - Returns json!({ action: "node_removed", node_id, run_id, removed_edges: count, metadata_only: true })\n\n' +
  'METHOD 3: update_workflow_node_status(run_id, node_id, new_status, actor, reason) -> Result<Value, String>\n' +
  '  - Validates status is one of: pending, running, completed, failed, cancelled, blocked, waiting_human\n' +
  '  - Updates both the status column and the node_json status field, records dag.mutation.node_status_updated event\n\n' +
  'METHOD 4: insert_workflow_edge(run_id, edge: &Value, actor, reason) -> Result<Value, String>\n' +
  '  - Checks run exists, checks for duplicate edge_id, inserts edge, records dag.mutation.edge_added event\n\n' +
  'METHOD 5: remove_workflow_edge(run_id, edge_id, actor, reason) -> Result<Value, String>\n' +
  '  - Deletes the edge, records dag.mutation.edge_removed event\n\n' +
  'METHOD 6: rewire_workflow_edge(run_id, edge_id, new_from: Option<&str>, new_to: Option<&str>, actor, reason) -> Result<Value, String>\n' +
  '  - Reads current from/to, applies changes (keeping current if None), updates edge_json, records dag.mutation.edge_rewired event\n\n' +
  'METHOD 7: apply_dag_mutations_batch(run_id, proposals: &[DAGMutationProposal], actor) -> Result<Vec<Value>, String>\n' +
  '  - Rebuilds a DAGManager from persisted nodes/edges, applies each proposal,\n' +
  '  - On success: persists mutation to SQLite via the appropriate method (insert/remove/update/rewire),\n' +
  '  - Records dag.mutation.applied event for each, returns vec of results\n' +
  '  - Import: use crate::workflow::dag_manager::{DAGManager, types::DAGMutationProposal}\n\n' +
  'METHOD 8: replay_mutation_events(run_id) -> Result<Value, String>\n' +
  '  - Gets base nodes/edges from run, then replays all dag.mutation.* events in order\n' +
  '  - Handles: node_added, node_removed, node_status_updated, edge_added, edge_removed, edge_rewired\n' +
  '  - Identifies completed/failed nodes from base state and marks them as protected_from_rerun after replay\n' +
  '  - Returns json!({ run_id, mutations_replayed, nodes, edges, protected_completed_nodes, metadata_only: true })\n\n' +
  'CRITICAL RULES:\n' +
  '- Do NOT modify any existing code. Only ADD new methods.\n' +
  '- Use the existing helper functions: insert_workflow_run_node_locked, insert_workflow_run_edge_locked,\n' +
  '  insert_workflow_run_event_locked, ensure_run_exists_locked, get_run_row, workflow_run_events_locked\n' +
  '- Use existing imports: use serde_json::{json, Value}; use rusqlite::params;\n' +
  '- Add the DAGManager import at the top of the file if not already present.\n\n' +
  'After writing, verify compilation:\n' +
  'cd /home/igzela/Projects/token-efficient-agent-harness-lab && cargo check -p engine 2>&1 | tail -20',
  { label: 'implement:mutation-storage', phase: 'Implement', model: 'opus' }
)

phase('Test')

const task_2b = await agent(
  'You are writing tests for the Persisted Graph Mutation Runtime feature.\n\n' +
  'STEP 1: Read the existing test patterns:\n' +
  '/home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/workflow_runs.rs (look at the tests at the bottom of the file for patterns)\n\n' +
  'STEP 2: Create the test file at:\n' +
  '/home/igzela/Projects/token-efficient-agent-harness-lab/engine/src/storage/local_product_store/workflow_runs_mutation_tests.rs\n\n' +
  'STEP 3: Add the test module declaration. Find the local_product_store mod.rs file and check how test modules are included. If there is no pattern for test file inclusion, add this to the bottom of mod.rs:\n' +
  '#[cfg(test)]\nmod workflow_runs_mutation_tests;\n\n' +
  'TEST HELPER:\n' +
  'fn new_test_store() -> LocalProductStore {\n' +
  '    LocalProductStore::new(":memory:").expect("failed to create test store")\n' +
  '}\n\n' +
  'fn create_test_plan(store: &LocalProductStore) -> Value {\n' +
  '    store.create_workflow_plan(\n' +
  '        "test-req", "test", "test-workflow", "test-dispatch",\n' +
  '        &json!({\n' +
  '            "workflow_id": "test-workflow",\n' +
  '            "dispatch_id": "test-dispatch",\n' +
  '            "nodes": [\n' +
  '                {"node_id": "node-a", "task_type": "implementation", "status": "pending"},\n' +
  '                {"node_id": "node-b", "task_type": "testing", "status": "pending"}\n' +
  '            ],\n' +
  '            "edges": [\n' +
  '                {"edge_id": "edge-ab", "from_node_id": "node-a", "to_node_id": "node-b", "edge_type": "dependency"}\n' +
  '            ]\n' +
  '        }),\n' +
  '    ).expect("failed to create plan")\n' +
  '}\n\n' +
  'fn run_from_plan(store: &LocalProductStore) -> Value {\n' +
  '    let plan = create_test_plan(store);\n' +
  '    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();\n' +
  '    store.create_workflow_run_from_plan(plan_id, "test").unwrap()\n' +
  '}\n\n' +
  'TESTS TO WRITE (one per function):\n\n' +
  '1. test_insert_workflow_node_success - insert node-c, verify run has 3 nodes\n' +
  '2. test_insert_workflow_node_duplicate_fails - insert node-a again, expect error\n' +
  '3. test_insert_workflow_node_records_event - verify dag.mutation.node_added event exists\n' +
  '4. test_remove_workflow_node_success - remove node-b, verify 1 node left, 0 edges\n' +
  '5. test_remove_workflow_node_not_found_fails - remove nonexistent, expect error\n' +
  '6. test_remove_workflow_node_records_event - verify dag.mutation.node_removed event\n' +
  '7. test_update_workflow_node_status_success - update node-a to completed\n' +
  '8. test_update_workflow_node_status_invalid_fails - set status to "bogus", expect error\n' +
  '9. test_update_workflow_node_status_not_found_fails\n' +
  '10. test_insert_workflow_edge_success - add edge-ac, verify 2 edges\n' +
  '11. test_insert_workflow_edge_duplicate_fails - add edge-ab again, expect error\n' +
  '12. test_remove_workflow_edge_success - remove edge-ab, verify 0 edges\n' +
  '13. test_remove_workflow_edge_not_found_fails\n' +
  '14. test_rewire_workflow_edge_success - add node-c, rewire edge-ab from->node-c, verify\n' +
  '15. test_rewire_workflow_edge_partial - only change target, keep source\n' +
  '16. test_replay_mutation_events_no_mutations - replay fresh run, 0 mutations\n' +
  '17. test_replay_mutation_events_with_add_and_remove - add node-c, remove node-b, replay\n' +
  '18. test_replay_protects_completed_nodes - complete node-a, mutate, verify protected\n' +
  '19. test_replay_with_edge_rewire - add node-c, rewire edge, verify replay\n' +
  '20. test_apply_dag_mutations_batch_add_node - use DAGMutationProposal to add node\n' +
  '21. test_apply_dag_mutations_batch_records_events - verify dag.mutation.applied events\n' +
  '22. test_export_import_roundtrip_with_mutations - mutate, export, import, verify\n' +
  '23. test_integrity_after_mutations - mutate, check_integrity, verify ok\n' +
  '24. test_no_duplicate_execution_after_mutation - tick node-a, add node-c, tick again, node-a not re-executed\n' +
  '25. test_workflow_run_detail_includes_mutation_events - verify get_workflow_run returns mutation events\n\n' +
  'For test 20, use:\n' +
  'use crate::workflow::dag_manager::types::DAGMutationProposal;\n' +
  'use std::collections::HashMap;\n\n' +
  'After writing, run:\n' +
  'cd /home/igzela/Projects/token-efficient-agent-harness-lab && cargo test -p engine --lib -- workflow_runs_mutation_tests 2>&1 | tail -40',
  { label: 'test:mutation-tests', phase: 'Test', model: 'opus' }
)

phase('Verify')

const verify_result = await agent(
  'Run full verification suite after Batch 1 changes.\n\n' +
  'Run these commands sequentially and report ALL results:\n\n' +
  '1. cd /home/igzela/Projects/token-efficient-agent-harness-lab && cargo test -p engine 2>&1 | tail -50\n' +
  '   Report: total test count, any failures\n\n' +
  '2. cd /home/igzela/Projects/token-efficient-agent-harness-lab && cargo clippy -p engine --all-targets -- -D warnings 2>&1 | tail -30\n' +
  '   Report: any warnings or errors\n\n' +
  '3. cd /home/igzela/Projects/token-efficient-agent-harness-lab && uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n' +
  '   Report: handoff check status\n\n' +
  'If any step fails, fix the issue and re-run. Report final status.',
  { label: 'verify:full-suite', phase: 'Verify', model: 'sonnet' }
)

log('Batch 1 implementation complete. Verification: ' + verify_result)
return { batch1: 'complete', verification: verify_result }
