export const meta = {
  name: 'on-1-tab-grouping-welcome',
  description: 'ON-1: Tab grouping (Monitor/System/Admin) + Welcome Panel',
  phases: [
    { title: 'Implement', detail: 'Create TabGroup.tsx, WelcomePanel.tsx, update page.tsx' },
    { title: 'Verify', detail: 'Build dashboard and screenshot' },
  ],
};

phase('Implement');

await agent('Create dashboard/src/components/TabGroup.tsx: a component that groups tabs into sections (Monitor, System, Admin). Monitor: mission, dispatches, routing, decisions, costs. System: scheduler, pool, queue, runs, patches. Admin: team, settings, health, backups, audit, operations. Each group has a label and collapsible "More" button for System/Admin groups. Use existing .tab CSS class. Accept tabs array, active tab, and onTabChange callback as props.', { label: 'TabGroup', model: 'opus' });

await agent('Create dashboard/src/components/WelcomePanel.tsx: a dismissible welcome panel shown when dispatches=0. Contains 3-step curl walkthrough: (1) Start engine, (2) Create noop dispatch, (3) View dispatch. Uses localStorage key "acp-welcome-dismissed" to persist dismissal. Uses existing .command-block and .empty-state CSS classes. Accept onDismiss callback prop.', { label: 'WelcomePanel', model: 'opus' });

await agent('Update dashboard/src/app/page.tsx: (1) Import TabGroup and WelcomePanel. (2) Replace flat tab nav with TabGroup component, passing grouped tabs structure. (3) Add WelcomePanel above status-strip, visible only when dashboard.counts.dispatches === 0 and welcome not dismissed. (4) Keep all existing functionality intact (auth, refresh, theme, etc). Read the file first, then make targeted edits.', { label: 'page.tsx update', model: 'opus' });

phase('Verify');

await agent('cd /home/igzela/Projects/token-efficient-agent-harness-lab/dashboard && ACP_DASHBOARD_OUTPUT=export npx next build 2>&1 | tail -30', { label: 'build verify', model: 'sonnet' });
