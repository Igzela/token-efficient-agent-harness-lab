export const meta = {
  name: 'batch5-agent-profiles',
  description: 'Add reusable agent profiles for planner, implementer, reviewer, tester, researcher with tools, model, context_budget, workspace_scope',
  phases: [
    { title: 'Research', detail: 'Explore existing agent role registry and node executor' },
    { title: 'Implement', detail: 'Write AgentProfile system and tests' },
    { title: 'Verify', detail: 'Run tests, clippy, handoff check' },
    { title: 'Ship', detail: 'Commit, push, CI check, docs update, memory update' },
  ],
}

phase('Research')

const researchResult = await agent(
  'Read these files and summarize what exists for Batch 5 Agent Profiles:\n' +
  '1. engine/src/orchestration/agent_role_registry.rs - current AgentRoleRegistry\n' +
  '2. engine/src/orchestration/schemas.rs - WorkflowNode, agent-related fields\n' +
  '3. engine/src/node_executor/mod.rs - NodeExecutor trait, NodeExecutionInput\n' +
  '4. engine/src/workflow/dynamic_controller.rs - how nodes are executed\n' +
  '5. engine/src/cli/mod.rs - CliNodeExecutor, CliConfig\n\n' +
  'Output:\n' +
  '- What AgentRoleRegistry currently stores\n' +
  '- What fields WorkflowNode has for agent/executor info\n' +
  '- What NodeExecutionInput passes to executors\n' +
  '- What agent profiles should add: tools, model, context_budget, workspace_scope\n' +
  '- How profiles map to executor selection',
  { label: 'research', phase: 'Research' }
)

log('Research: ' + (researchResult || '').substring(0, 200))

phase('Implement')

const profilesImpl = await agent(
  'Implement Batch 5: Agent Profiles in engine/src/workflow/agent_profiles.rs.\n\n' +
  'Current state: AgentRoleRegistry exists but is not deeply wired. Nodes have task_type but no reusable profile.\n' +
  'Goal: Add first-class agent profiles that map to executor selection, tools, model, context budget, workspace scope.\n\n' +
  'New file: engine/src/workflow/agent_profiles.rs\n\n' +
  'Key types:\n' +
  '- AgentProfileId: String newtype\n' +
  '- AgentProfile: profile_id, role (planner/implementer/reviewer/tester/researcher), tools (Vec of String), model_hint (Option), context_budget_tokens (Option u64), workspace_scope (enum: full/task/isolated), executor_preference (Option String), max_retries (u32)\n' +
  '- AgentProfileRegistry: HashMap of profile_id -> AgentProfile, with default profiles\n\n' +
  'Built-in default profiles:\n' +
  '- planner: role=planner, tools=[read, analyze], context_budget=20000, workspace_scope=full\n' +
  '- implementer: role=implementer, tools=[read, write, edit, bash], context_budget=40000, workspace_scope=task\n' +
  '- reviewer: role=reviewer, tools=[read, comment], context_budget=20000, workspace_scope=full\n' +
  '- tester: role=tester, tools=[read, bash, write], context_budget=30000, workspace_scope=task\n' +
  '- researcher: role=researcher, tools=[read, search], context_budget=15000, workspace_scope=full\n\n' +
  'SQLite table (add to LocalProductStore DDL):\n' +
  '  agent_profiles (\n' +
  '    profile_id TEXT PRIMARY KEY,\n' +
  '    role TEXT NOT NULL,\n' +
  '    tools_json TEXT NOT NULL,\n' +
  '    model_hint TEXT,\n' +
  '    context_budget_tokens INTEGER,\n' +
  '    workspace_scope TEXT NOT NULL DEFAULT task,\n' +
  '    executor_preference TEXT,\n' +
  '    max_retries INTEGER NOT NULL DEFAULT 3,\n' +
  '    created_at TEXT NOT NULL,\n' +
  '    updated_at TEXT NOT NULL\n' +
  '  )\n\n' +
  'Storage methods on LocalProductStore:\n' +
  '- upsert_agent_profile(profile_id, role, tools, model_hint, context_budget, workspace_scope, executor_preference, max_retries)\n' +
  '- get_agent_profile(profile_id) -> Option<AgentProfile>\n' +
  '- list_agent_profiles() -> Vec<AgentProfile>\n' +
  '- delete_agent_profile(profile_id)\n' +
  '- get_profile_for_role(role) -> Option<AgentProfile> (returns first match for role)\n\n' +
  'Integration:\n' +
  '- WorkflowRunNodes table gets profile_id column (nullable, ALTER TABLE)\n' +
  '- DynamicWorkflowController: when creating fix/test/review nodes, attach profile_id based on role\n' +
  '- DynamicDecomposer: NodeProposal gets profile_id field, propagated to DAGMutationProposal payload\n' +
  '- tick_with_executor_and_command: if node has profile_id, use profile executor_preference and context_budget\n\n' +
  'New file: engine/src/workflow/agent_profiles_tests.rs\n' +
  'Required tests:\n' +
  '1. test_default_profiles_exist\n' +
  '2. test_upsert_and_get_profile\n' +
  '3. test_list_profiles\n' +
  '4. test_delete_profile\n' +
  '5. test_get_profile_for_role\n' +
  '6. test_node_records_profile_id\n' +
  '7. test_controller_attaches_fix_profile\n' +
  '8. test_controller_attaches_review_profile\n' +
  '9. test_decomposer_attaches_profile_to_proposals\n' +
  '10. test_export_import_round_trip\n\n' +
  'Add mod in engine/src/workflow/mod.rs:\n' +
  '  pub mod agent_profiles;\n' +
  '  #[cfg(test)] mod agent_profiles_tests;\n\n' +
  'Constraints:\n' +
  '- Do NOT create parallel scheduler/runtime/DAG kernel\n' +
  '- Do NOT enable provider execution\n' +
  '- First version runs serially; fan-out controlled by existing scheduler limits\n' +
  '- All tests use in-memory store',
  { label: 'profiles-impl', phase: 'Implement', model: 'opus' }
)

log('Profiles impl: ' + (profilesImpl || '').substring(0, 200))

phase('Verify')

const verifyResult = await agent(
  'Run these verification commands and report results:\n\n' +
  '1. cargo test -p engine --lib workflow::agent_profiles 2>&1 | tail -30\n' +
  '2. cargo test -p engine 2>&1 | tail -5\n' +
  '3. cargo clippy -p engine -- -D warnings 2>&1 | tail -20\n' +
  '4. uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n\n' +
  'If any test fails, fix and re-run. Report: test count, pass/fail, clippy, handoff.',
  { label: 'verify', phase: 'Verify' }
)

log('Verify: ' + (verifyResult || '').substring(0, 300))

phase('Ship')

const shipResult = await agent(
  'Complete the shipping process for Batch 5:\n\n' +
  '1. Get current test count:\n' +
  '   cargo test -p engine 2>&1 | grep -E "test result:|passed"\n\n' +
  '2. Update docs/CURRENT_STATUS.md:\n' +
  '   - Update test count in Repository State section\n' +
  '   - Update Dynamic Workflow Batch line to include Batch 5\n' +
  '   - Update Dynamicity assessment row\n\n' +
  '3. Update docs/NEXT_DECISION.md:\n' +
  '   - Mark Batch 5 as DONE in the batches table\n' +
  '   - Update "Current status" line\n' +
  '   - Mark Batch 6 as next\n\n' +
  '4. Update docs/MODULE_MAP.md:\n' +
  '   - Update Agent Profiles row to DONE\n\n' +
  '5. Update CLAUDE.md test count\n\n' +
  '6. Update AGENTS.md test count\n\n' +
  '7. Commit all changes:\n' +
  '   git add -A\n' +
  '   git commit -m "Batch 5: Agent Profiles — planner/implementer/reviewer/tester/researcher profiles, tools, context_budget, workspace_scope (11XX tests)"\n\n' +
  '8. Push:\n' +
  '   git push\n\n' +
  '9. Check CI status (wait up to 5 minutes, check every 30s):\n' +
  '   gh run list --branch feat/dashboard-ux-polish --limit 1 --json status,conclusion\n' +
  '   If conclusion != "success", get the log and fix the issue:\n' +
  '   gh run view <run_id> --log-failed\n' +
  '   Fix the failing test/lint, commit, push again, re-check.\n\n' +
  '10. Write memory file at /home/igzela/.claude/projects/-home-igzela-Projects-token-efficient-agent-harness-lab/memory/project_dynamic_workflow_batch5_closeout.md\n\n' +
  '11. Update MEMORY.md index with new entry.\n\n' +
  'Report final status: commit hash, test count, CI status.',
  { label: 'ship', phase: 'Ship' }
)

log('Ship: ' + (shipResult || '').substring(0, 300))

return {
  batch: 'Batch 5: Agent Profiles',
  implementation: profilesImpl ? 'done' : 'failed',
  verification: verifyResult || 'unknown',
  shipping: shipResult || 'unknown',
}
