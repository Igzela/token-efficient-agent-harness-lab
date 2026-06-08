export const meta = {
  name: 'on-3-term-tooltip',
  description: 'ON-3: Term tooltip system for jargon terms',
  phases: [
    { title: 'Create', detail: 'Create TermTooltip.tsx component' },
    { title: 'Apply', detail: 'Add tooltips to key terms across components' },
    { title: 'Verify', detail: 'Build dashboard' },
  ],
};

phase('Create');

await agent('Create dashboard/src/components/TermTooltip.tsx: A component that wraps text with a tooltip showing a one-line definition. Props: term (string key), children. Use a terms object mapping term keys to definitions. Terms to define: dispatch="A single task routed through the harness pipeline", tier="Model capability level (cheap/balanced/strong)", confidence="How certain the routing decision is about task complexity", backpressure="System throttle when too many tasks are queued", degrade_mode="Fallback mode when primary path is unavailable", executor="Component that runs a dispatched task", run="A workflow execution instance with nodes and events", decision="A routing or policy choice made by the orchestrator", noop="No-operation execution for testing and safety", budget="Resource limits for token usage and cost per dispatch". Use CSS class .term-tooltip with hover reveal. Use a <span> with title attribute for simplicity (no extra CSS needed, browsers show native tooltips).', { label: 'TermTooltip component', model: 'opus' });

phase('Apply');

await parallel([
  () => agent('Update dashboard/src/components/BoundaryBadges.tsx: Import TermTooltip. Wrap key badge labels with TermTooltip where relevant: "Providers" -> term="tier", "Target writes" -> term="executor", "Sandbox" -> term="executor", "Workers" -> term="executor". Read the file first, then make targeted edits.', { label: 'BoundaryBadges tooltips', model: 'opus' }),
  () => agent('Update dashboard/src/app/page.tsx: Import TermTooltip. In the status strip, wrap "Dispatches" label with TermTooltip term="dispatch", and "Cost" label with TermTooltip term="budget". In the setup checklist, wrap "Admin key available" with a subtle note. Read the file first, then make targeted edits to the status-strip section only.', { label: 'page.tsx tooltips', model: 'opus' }),
  () => agent('Update dashboard/src/components/Health.tsx: Import TermTooltip. Wrap "API" label with TermTooltip term="dispatch", wrap "Readiness" with a generic tooltip. Wrap boundary keys with human-readable names using TermTooltip. Read the file first, then make targeted edits.', { label: 'Health tooltips', model: 'opus' }),
  () => agent('Update dashboard/src/components/DecisionLog.tsx: Import TermTooltip. In the StatsTiles, wrap "Total decisions" with TermTooltip term="decision", wrap "Avg confidence" with TermTooltip term="confidence". In the table headers, wrap "Action" with TermTooltip term="decision", "Executor" with TermTooltip term="executor", "Confidence" with TermTooltip term="confidence", "Tier" with TermTooltip term="tier". Read the file first, then make targeted edits.', { label: 'DecisionLog tooltips', model: 'opus' }),
]);

phase('Verify');

await agent('cd /home/igzela/Projects/token-efficient-agent-harness-lab/dashboard && ACP_DASHBOARD_OUTPUT=export npx next build 2>&1 | tail -20', { label: 'build verify', model: 'sonnet' });
