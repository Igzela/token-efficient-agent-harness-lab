export function Routing({ rows }: { rows: Array<{ confidence: number; fallback: string; group: string; selected: string }> }) {
  if (rows.length === 0) return <p className="muted">No routing decisions</p>;
  return (
    <section style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(3, minmax(0, 1fr))" }}>
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
