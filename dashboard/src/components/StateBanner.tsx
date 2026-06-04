import type { ReactNode } from "react";

export function StateBanner({
  actions,
  children,
  title,
  tone = "info",
}: {
  actions?: ReactNode;
  children: ReactNode;
  title: string;
  tone?: "info" | "ok" | "warn" | "risk";
}) {
  return (
    <aside className={`state-banner state-banner-${tone}`}>
      <span aria-hidden="true" className="state-mark" />
      <div className="state-banner-copy">
        <strong>{title}</strong>
        <div>{children}</div>
      </div>
      {actions && <div className="state-banner-actions">{actions}</div>}
    </aside>
  );
}
