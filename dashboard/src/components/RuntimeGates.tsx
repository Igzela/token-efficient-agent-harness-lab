import type { LocalBoundaries } from "@/lib/types";

type GateTone = "ok" | "warn" | "info";

type Gate = {
  detail: string;
  label: string;
  status: string;
  tone: GateTone;
};

function gateTone(value: string): GateTone {
  if (value === "enabled" || value === "provider/enabled") return "warn";
  if (value === "disabled" || value === "stub/off" || value === "local-only") return "ok";
  return "info";
}

function authGate(authStatus: "ok" | "missing" | "denied" | "offline", hasToken: boolean): Gate {
  if (authStatus === "offline") {
    return {
      detail: "Engine unreachable; start local runtime before protected actions are available.",
      label: "Auth",
      status: "offline",
      tone: "warn",
    };
  }
  if (authStatus === "missing") {
    return {
      detail: "Protected mode needs a local API key in this browser.",
      label: "Auth",
      status: "key needed",
      tone: "warn",
    };
  }
  if (authStatus === "denied") {
    return {
      detail: "Stored key was rejected or lacks the required scope for this view.",
      label: "Auth",
      status: "denied",
      tone: "warn",
    };
  }
  return {
    detail: hasToken ? "Local API key stored for protected endpoints." : "Open local mode; protected auth is not required by this engine.",
    label: "Auth",
    status: hasToken ? "key stored" : "open local",
    tone: "ok",
  };
}

export function RuntimeGates({
  authStatus,
  boundaries,
  hasToken,
}: {
  authStatus: "ok" | "missing" | "denied" | "offline";
  boundaries: LocalBoundaries;
  hasToken: boolean;
}) {
  const gates: Gate[] = [
    authGate(authStatus, hasToken),
    {
      detail: boundaries.provider_transport === "provider/enabled"
        ? "Provider transport is explicitly enabled for this local runtime."
        : "Real provider calls are off; dispatches stay on noop or explicit opt-in paths.",
      label: "Provider",
      status: boundaries.provider_transport,
      tone: gateTone(boundaries.provider_transport),
    },
    {
      detail: boundaries.runtime_workers === "enabled"
        ? "Runtime worker capability is enabled by explicit local configuration."
        : "CLI-backed execution remains off/default-safe unless locally enabled.",
      label: "CLI",
      status: boundaries.runtime_workers,
      tone: gateTone(boundaries.runtime_workers),
    },
    {
      detail: boundaries.target_repository_writes === "disabled"
        ? "Workspace controls operate on app-owned detached workspaces, not target repositories."
        : "Review workspace boundary before using app-owned controls.",
      label: "Workspace",
      status: boundaries.target_repository_writes === "disabled" ? "app-owned only" : boundaries.target_repository_writes,
      tone: gateTone(boundaries.target_repository_writes),
    },
    {
      detail: "Artifact export requires approval binding and preserves the target-write boundary.",
      label: "Export",
      status: "approval-bound",
      tone: "ok",
    },
  ];

  return (
    <section className="setup-card" aria-label="Runtime gates">
      <div className="setup-heading">
        <div>
          <p className="label">Runtime gates</p>
          <h2>Provider, CLI, auth, workspace, and export status</h2>
        </div>
        <span className="pill info">guarded</span>
      </div>
      <div className="metrics">
        {gates.map((gate) => (
          <div className="metric" key={gate.label}>
            <span className="metric-label">{gate.label}</span>
            <strong>{gate.status}</strong>
            <span className={gate.tone}>{gate.detail}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
