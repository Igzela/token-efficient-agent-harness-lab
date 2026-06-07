import type { LocalBoundaries } from "@/lib/types";

const boundaryLabels: Array<[keyof LocalBoundaries, string]> = [
  ["deployment", "Deployment"],
  ["provider_transport", "Providers"],
  ["target_repository_writes", "Target writes"],
  ["sandbox_process_execution", "Sandbox"],
  ["runtime_workers", "Workers"],
];

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
      {boundaryLabels.map(([key, label]) => (
        <span className="boundary-badge" key={key}>
          {label}: {String(boundaries[key])}
        </span>
      ))}
    </div>
  );
}
