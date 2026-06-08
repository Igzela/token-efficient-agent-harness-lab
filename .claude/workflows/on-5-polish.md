export const meta = {
  name: 'on-5-polish',
  description: 'ON-5: Polish — per-tab last updated, setup commands, settings links, docs link, search hints',
  phases: [
    { title: 'Polish', detail: 'Per-tab last updated, setup commands, settings env vars, docs link, search hints' },
    { title: 'Verify', detail: 'Build dashboard + final verification suite' },
  ],
};

phase('Polish');

await parallel([
  () => agent('Update dashboard/src/app/page.tsx: (1) Add a "Docs" link in the topbar-meta section, linking to the GitHub repo README. Use a simple <a> tag with className="topbar-btn" href="https://github.com/anthropics/agent-control-plane" target="_blank" rel="noopener noreferrer". (2) In the setup checklist steps, add the actual commands for each step where missing: "Engine reachable" already has context, "Runtime ready" add "Check scheduler status in the Scheduler tab", "Admin key available" add "Set ACP_ADMIN_TOKEN env var", "First dispatch recorded" add "Use the curl command in the Dispatches tab", "Team boundary configured" add "Configure in the Team tab". Read the file first, then make targeted edits.', { label: 'docs link + setup cmds', model: 'opus' }),
  () => agent('Update dashboard/src/components/Settings.tsx: Add a section below "Provider Health" that lists the key environment variables the system uses. Show them as a readable list with variable name and description. Key env vars: ACP_ADMIN_TOKEN (Admin API key), ACP_REQUIRE_AUTH (Enable auth), ACP_PROVIDER_TYPE (Provider adapter), ACP_ENABLE_PROVIDER_EXECUTION (Enable real providers), ACP_DATABASE_URL (PostgreSQL backend), ACP_DB_PATH (SQLite path), ACP_BACKUP_INTERVAL_SEC (Backup interval), ACP_TLS_CERT_PATH/TLS_KEY_PATH (TLS), ACP_DB_ENCRYPTION_KEY (Encryption). Use the existing .readable-list and .kv-row CSS classes. Read the file first, then make the edit.', { label: 'settings env vars', model: 'opus' }),
  () => agent('Update dashboard/src/components/SearchBar.tsx: Add a placeholder hint text that is more descriptive. Read the file first to understand the current structure, then confirm if the placeholder is already good or suggest an improvement.', { label: 'search hints', model: 'sonnet' }),
]);

phase('Verify');

await agent('cd /home/igzela/Projects/token-efficient-agent-harness-lab/dashboard && ACP_DASHBOARD_OUTPUT=export npx next build 2>&1 | tail -20', { label: 'build verify', model: 'sonnet' });
