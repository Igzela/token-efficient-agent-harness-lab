const terms: Record<string, string> = {
  dispatch: "A single task routed through the harness pipeline",
  tier: "Model capability level (cheap/balanced/strong)",
  confidence: "How certain the routing decision is about task complexity",
  backpressure: "System throttle when too many tasks are queued",
  degrade_mode: "Fallback mode when primary path is unavailable",
  executor: "Component that runs a dispatched task",
  run: "A workflow execution instance with nodes and events",
  decision: "A routing or policy choice made by the orchestrator",
  noop: "No-operation execution for testing and safety",
  budget: "Resource limits for token usage and cost per dispatch",
};

export function TermTooltip({
  term,
  children,
}: {
  term: string;
  children?: React.ReactNode;
}) {
  const definition = terms[term] ?? term;
  return (
    <span title={definition} style={{ borderBottom: "1px dotted var(--muted)", cursor: "help" }}>
      {children ?? term}
    </span>
  );
}
