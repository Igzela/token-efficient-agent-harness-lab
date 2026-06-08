export const meta = {
  name: 'on-2-empty-state-overhaul',
  description: 'ON-2: Empty state overhaul with concrete next-step actions',
  phases: [
    { title: 'Overhaul', detail: 'Update all EmptyState usages with actionable next steps' },
    { title: 'Verify', detail: 'Build dashboard and screenshot' },
  ],
};

phase('Overhaul');

await parallel([
  () => agent('Update dashboard/src/components/Dispatches.tsx: The EmptyState for "No matching dispatches" is fine. The EmptyState for "No dispatch history yet" already has a curl command - good. The EmptyState for "No gate records" should include: "Create a dispatch to see quality gate records." with the same noopDispatchCommand curl block. Read the file first, then update only the Quality Gates empty state.', { label: 'Dispatches empty', model: 'opus' }),
  () => agent('Update dashboard/src/components/DecisionLog.tsx: The EmptyState for "No decisions recorded" should include: "Send a dispatch through the API to see routing decisions." and show the curl command: curl -X POST http://127.0.0.1:9999/api/v1/dispatch -H "content-type: application/json" -d \'{"raw_request":"Review docs","request_source":"manual"}\'. Read the file first, then update the EmptyState JSX.', { label: 'DecisionLog empty', model: 'opus' }),
  () => agent('Update dashboard/src/components/WorkflowRuns.tsx: The EmptyState for "No workflow runs" should include: "Create a plan via the API to start a workflow run." with curl command: curl -X POST http://127.0.0.1:9999/api/v1/plans -H "content-type: application/json" -d \'{"raw_request":"Implement feature X","request_source":"manual"}\' then create a run from it. Read the file first, then update the EmptyState JSX.', { label: 'WorkflowRuns empty', model: 'opus' }),
  () => agent('Update dashboard/src/components/MissionControl.tsx: The EmptyState for "No workflow runs" should include: "Create a plan to populate mission control state." with curl command: curl -X POST http://127.0.0.1:9999/api/v1/plans -H "content-type: application/json" -d \'{"raw_request":"Implement feature","request_source":"manual"}\'. Read the file first, then update the EmptyState JSX.', { label: 'MissionControl empty', model: 'opus' }),
  () => agent('Update dashboard/src/components/Settings.tsx: The EmptyState for "No local config overrides" is already good (info tone). No change needed for Settings. Just verify it looks correct and confirm.', { label: 'Settings verify', model: 'sonnet' }),
]);

phase('Verify');

await agent('cd /home/igzela/Projects/token-efficient-agent-harness-lab/dashboard && ACP_DASHBOARD_OUTPUT=export npx next build 2>&1 | tail -30', { label: 'build verify', model: 'sonnet' });
