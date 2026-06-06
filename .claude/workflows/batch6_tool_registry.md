export const meta = {
  name: 'batch6-tool-registry',
  description: 'Tool registry and hook points: tool capability, allowlist, pre/post execution hooks, MCP-like descriptors',
  phases: [
    { title: 'Research', detail: 'Explore existing infrastructure and hook patterns' },
    { title: 'Implement', detail: 'Write ToolRegistry and tests' },
    { title: 'Verify', detail: 'Run tests, clippy, handoff check' },
    { title: 'Ship', detail: 'Commit, push, CI check, docs update, memory update' },
  ],
}

phase('Research')

const researchResult = await agent(
  'Read these files and summarize what exists for Batch 6 Tool Registry:\n' +
  '1. engine/src/infrastructure/ - existing infrastructure modules\n' +
  '2. engine/src/node_executor/mod.rs - NodeExecutor trait, what tools executors use\n' +
  '3. engine/src/cli/mod.rs - CliNodeExecutor, allowedTools\n' +
  '4. engine/src/workflow/agent_profiles.rs - AgentProfile tools field\n' +
  '5. engine/src/http_server/mod.rs - existing API routes pattern\n\n' +
  'Output:\n' +
  '- What tool execution paths exist today\n' +
  '- What hook points would be useful (pre/post execution)\n' +
  '- How MCP-like tool descriptors should be structured\n' +
  '- What SQLite tables are needed\n' +
  '- How hooks can block, enrich, or require approval',
  { label: 'research', phase: 'Research' }
)

log('Research: ' + (researchResult || '').substring(0, 200))

phase('Implement')

const registryImpl = await agent(
  'Implement Batch 6: Tool Registry and Hook Points in engine/src/workflow/tool_registry.rs.\n\n' +
  'Current state: Tools are hardcoded in NodeExecutor implementations. No registry, no hooks, no capability metadata.\n' +
  'Goal: App-owned metadata for tool capabilities, allowlists, pre/post execution hooks, and MCP-like descriptors.\n\n' +
  'New file: engine/src/workflow/tool_registry.rs\n\n' +
  'Key types:\n' +
  '- ToolCapability: name, description, input_schema (Value), output_schema (Value), requires_approval (bool), risk_level (low/medium/high)\n' +
  '- ToolAllowlist: profile_id -> allowed tool names\n' +
  '- HookType enum: PreExecution, PostExecution\n' +
  '- HookResult enum: Allow, Block(reason), Enrich(modified_input), RequestApproval(reason)\n' +
  '- ToolHook: hook_id, hook_type, tool_name (Option for all-tools), condition (Option Value), action (enum: log/block/enrich/request_approval)\n' +
  '- ToolDescriptor: MCP-like metadata (name, description, inputSchema, annotations)\n\n' +
  'SQLite tables (add to LocalProductStore DDL):\n' +
  '  tool_capabilities (\n' +
  '    tool_name TEXT PRIMARY KEY,\n' +
  '    description TEXT NOT NULL,\n' +
  '    input_schema_json TEXT,\n' +
  '    output_schema_json TEXT,\n' +
  '    requires_approval INTEGER NOT NULL DEFAULT 0,\n' +
  '    risk_level TEXT NOT NULL DEFAULT low,\n' +
  '    created_at TEXT NOT NULL\n' +
  '  )\n\n' +
  '  tool_allowlists (\n' +
  '    profile_id TEXT NOT NULL,\n' +
  '    tool_name TEXT NOT NULL,\n' +
  '    created_at TEXT NOT NULL,\n' +
  '    PRIMARY KEY (profile_id, tool_name)\n' +
  '  )\n\n' +
  '  tool_hooks (\n' +
  '    hook_id TEXT PRIMARY KEY,\n' +
  '    hook_type TEXT NOT NULL,\n' +
  '    tool_name TEXT,\n' +
  '    condition_json TEXT,\n' +
  '    action TEXT NOT NULL,\n' +
  '    action_config_json TEXT,\n' +
  '    enabled INTEGER NOT NULL DEFAULT 1,\n' +
  '    created_at TEXT NOT NULL\n' +
  '  )\n\n' +
  'Storage methods on LocalProductStore:\n' +
  '- register_tool_capability(name, description, input_schema, output_schema, requires_approval, risk_level)\n' +
  '- get_tool_capability(name) -> Option\n' +
  '- list_tool_capabilities() -> Vec\n' +
  '- set_tool_allowlist(profile_id, tool_names)\n' +
  '- check_tool_allowed(profile_id, tool_name) -> bool (true if no allowlist exists for profile, or tool is in list)\n' +
  '- add_tool_hook(hook_id, hook_type, tool_name, condition, action, action_config)\n' +
  '- evaluate_hooks(hook_type, tool_name, context) -> HookResult\n' +
  '- get_mcp_descriptors() -> Vec<ToolDescriptor> (converts capabilities to MCP-like format)\n\n' +
  'Hook evaluation logic:\n' +
  '- Get all enabled hooks matching hook_type and tool_name (or all-tools if tool_name is None)\n' +
  '- Evaluate in order: first Block wins, first RequestApproval wins, Enrich accumulates\n' +
  '- Default: Allow\n\n' +
  'Integration:\n' +
  '- NodeExecutor: before executing, call evaluate_hooks(PreExecution, tool_name, input)\n' +
  '- After executing, call evaluate_hooks(PostExecution, tool_name, output)\n' +
  '- AgentProfile tools checked against allowlist before execution\n' +
  '- requires_approval tools trigger ApprovalRequested action in DynamicWorkflowController\n\n' +
  'New file: engine/src/workflow/tool_registry_tests.rs\n' +
  'Required tests:\n' +
  '1. test_register_and_get_capability\n' +
  '2. test_list_capabilities\n' +
  '3. test_allowlist_blocks_unknown_tool\n' +
  '4. test_allowlist_permits_listed_tool\n' +
  '5. test_no_allowlist_permits_all\n' +
  '6. test_hook_blocks_execution\n' +
  '7. test_hook_enriches_input\n' +
  '8. test_hook_requests_approval\n' +
  '9. test_hooks_evaluated_in_order\n' +
  '10. test_mcp_descriptors_format\n' +
  '11. test_disabled_hook_not_evaluated\n' +
  '12. test_export_import_round_trip\n\n' +
  'Add mod in engine/src/workflow/mod.rs:\n' +
  '  pub mod tool_registry;\n' +
  '  #[cfg(test)] mod tool_registry_tests;\n\n' +
  'Constraints:\n' +
  '- Do NOT create parallel scheduler/runtime/DAG kernel\n' +
  '- Do NOT enable provider execution\n' +
  '- Hooks must be audited and deterministic in tests\n' +
  '- All tests use in-memory store',
  { label: 'registry-impl', phase: 'Implement', model: 'opus' }
)

log('Registry impl: ' + (registryImpl || '').substring(0, 200))

phase('Verify')

const verifyResult = await agent(
  'Run these verification commands and report results:\n\n' +
  '1. cargo test -p engine --lib workflow::tool_registry 2>&1 | tail -30\n' +
  '2. cargo test -p engine 2>&1 | tail -5\n' +
  '3. cargo clippy -p engine -- -D warnings 2>&1 | tail -20\n' +
  '4. uv run --no-project python scripts/check_agent_handoff.py 2>&1 | tail -20\n\n' +
  'If any test fails, fix and re-run. Report: test count, pass/fail, clippy, handoff.',
  { label: 'verify', phase: 'Verify' }
)

log('Verify: ' + (verifyResult || '').substring(0, 300))

phase('Ship')

const shipResult = await agent(
  'Complete the shipping process for Batch 6:\n\n' +
  '1. Get current test count:\n' +
  '   cargo test -p engine 2>&1 | grep -E "test result:|passed"\n\n' +
  '2. Update docs/CURRENT_STATUS.md:\n' +
  '   - Update test count in Repository State section\n' +
  '   - Update Dynamic Workflow Batch line to include Batch 6\n' +
  '   - Update Dynamicity assessment row\n\n' +
  '3. Update docs/NEXT_DECISION.md:\n' +
  '   - Mark Batch 6 as DONE in the batches table\n' +
  '   - Update "Current status" line\n' +
  '   - Mark Batch 7 as next\n\n' +
  '4. Update docs/MODULE_MAP.md:\n' +
  '   - Update Tool Registry row to DONE\n\n' +
  '5. Update CLAUDE.md test count\n\n' +
  '6. Update AGENTS.md test count\n\n' +
  '7. Commit all changes:\n' +
  '   git add -A\n' +
  '   git commit -m "Batch 6: Tool Registry — capabilities, allowlists, pre/post hooks, MCP descriptors (11XX tests)"\n\n' +
  '8. Push:\n' +
  '   git push\n\n' +
  '9. Check CI status (wait up to 5 minutes, check every 30s):\n' +
  '   gh run list --branch feat/dashboard-ux-polish --limit 1 --json status,conclusion\n' +
  '   If conclusion != "success", get the log and fix the issue:\n' +
  '   gh run view <run_id> --log-failed\n' +
  '   Fix the failing test/lint, commit, push again, re-check.\n\n' +
  '10. Write memory file at /home/igzela/.claude/projects/-home-igzela-Projects-token-efficient-agent-harness-lab/memory/project_dynamic_workflow_batch6_closeout.md\n\n' +
  '11. Update MEMORY.md index with new entry.\n\n' +
  'Report final status: commit hash, test count, CI status.',
  { label: 'ship', phase: 'Ship' }
)

log('Ship: ' + (shipResult || '').substring(0, 300))

return {
  batch: 'Batch 6: Tool Registry',
  implementation: registryImpl ? 'done' : 'failed',
  verification: verifyResult || 'unknown',
  shipping: shipResult || 'unknown',
}
