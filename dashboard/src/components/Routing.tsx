import { EmptyState } from "./EmptyState";

export function Routing({ rows }: { rows: Array<{ confidence: number; fallback: string; group: string; selected: string }> }) {
  if (rows.length === 0) {
    return (
      <EmptyState
        title="No routing decisions yet"
        description="Routing choices are derived from local dispatch records. Create a dispatch to see selected tier, fallback tier, and confidence."
        tone="info"
      />
    );
  }
  return (
    <section className="routing-grid">
      {rows.map((r, i) => (
        <article className="card stack" key={i}>
          <div className="heading-row">
            <strong>{r.selected}</strong>
            <span className="pill info">{r.group}</span>
          </div>
          <span className="muted">confidence: {(r.confidence * 100).toFixed(0)}%</span>
          <span className="muted">fallback: {r.fallback}</span>
        </article>
      ))}
    </section>
  );
}
