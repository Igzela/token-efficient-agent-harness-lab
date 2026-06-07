import type { ReactNode } from "react";

export function EmptyState({
  actions,
  children,
  description,
  title,
  tone = "info",
}: {
  actions?: ReactNode;
  children?: ReactNode;
  description: string;
  title: string;
  tone?: "info" | "ok" | "warn" | "risk";
}) {
  return (
    <div className={`empty-state empty-state-${tone}`}>
      <span aria-hidden="true" className="state-mark" />
      <div className="empty-state-copy">
        <h3>{title}</h3>
        <p>{description}</p>
        {children}
        {actions && <div className="empty-state-actions">{actions}</div>}
      </div>
    </div>
  );
}
