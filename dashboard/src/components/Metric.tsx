import type { ReactNode } from "react";

export function Metric({
  detail,
  label,
  tone = "info",
  value,
}: {
  detail?: string;
  label: ReactNode;
  tone?: "ok" | "warn" | "info";
  value: string;
}) {
  return (
    <article className={`metric metric-${tone}`}>
      <span className="label">{label}</span>
      <strong>{value}</strong>
      <span className={tone}>{detail ?? tone}</span>
    </article>
  );
}
