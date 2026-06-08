export const meta = {
  name: 'on-4-copy-boundary',
  description: 'ON-4: Copy + boundary fixes for hero, badges, health',
  phases: [
    { title: 'Copy', detail: 'Rewrite hero, badges human-readable, health guidance' },
    { title: 'Verify', detail: 'Build dashboard' },
  ],
};

phase('Copy');

await parallel([
  () => agent('Update dashboard/src/app/page.tsx hero copy: Replace the current hero text "Monitor local dispatch history, team state, cost usage, audit events, and data operations without enabling cloud or target-repo execution paths." with something more user-friendly like: "A local control plane for studying agent workflows. Monitor dispatches, track costs, manage your team, and review audit history — all running on your machine." Keep it under 20 words. Read the file first, then make the edit.', { label: 'hero copy', model: 'opus' }),
  () => agent('Update dashboard/src/components/BoundaryBadges.tsx: Make badge values human-readable. "local-only" -> "Local", "noop" -> "Stub (testing)", "disabled" -> "Off", "enabled" -> "On". Also add hover tooltips to badges explaining what each means. Read the file first, then update the badge rendering logic.', { label: 'badge copy', model: 'opus' }),
  () => agent('Update dashboard/src/components/Health.tsx: The health guidance text is already added in ON-3. Verify the current state looks good and add a small note under the State Counts section: "These counts reflect persisted state in the local SQLite database." Read the file first, then make the edit.', { label: 'health guidance', model: 'sonnet' }),
]);

phase('Verify');

await agent('cd /home/igzela/Projects/token-efficient-agent-harness-lab/dashboard && ACP_DASHBOARD_OUTPUT=export npx next build 2>&1 | tail -20', { label: 'build verify', model: 'sonnet' });
