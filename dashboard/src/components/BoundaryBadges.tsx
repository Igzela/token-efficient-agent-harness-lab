import type { LocalBoundaries } from "@/lib/types";
import { TermTooltip } from "./TermTooltip";

const boundaryLabels: Array<[keyof LocalBoundaries, string, string?]> = [
  ["deployment", "Deployment"],
  ["provider_transport", "Providers", "tier"],
  ["target_repository_writes", "Target writes", "executor"],
  ["sandbox_process_execution", "Sandbox", "executor"],
  ["runtime_workers", "Workers", "executor"],
];

function humanize(value: unknown): string {
  const s = String(value);
  if (s === "local-only") return "Local";
  if (s === "noop" || s === "stub/off") return "Stub (testing)";
  if (s === "disabled") return "Off";
  if (s === "enabled") return "On";
  return s;
}

export function BoundaryBadges({
  authStatus,
  boundaries,
  hasToken,
}: {
  authStatus: "ok" | "missing" | "denied" | "offline";
  boundaries: LocalBoundaries;
  hasToken: boolean;
}) {
  const authLabel = authStatus === "offline"
    ? "Engine offline"
    : authStatus === "ok" && hasToken
      ? "API key stored"
      : authStatus === "ok"
        ? "Open local mode"
        : "API key needed";

  return (
    <div className="boundary-badges" aria-label="Local runtime boundaries">
      <span className={`boundary-badge ${authStatus === "ok" ? "ok" : "warn"}`}>
        Auth: {authLabel}
      </span>
      {boundaryLabels.map(([key, label, term]) => (
        <span className="boundary-badge" key={key}>
          {term ? <TermTooltip term={term}>{label}</TermTooltip> : label}: {humanize(boundaries[key])}
        </span>
      ))}
    </div>
  );
}
