"use client";

import { useState } from "react";

const STORAGE_KEY = "acp-welcome-dismissed";

function isDismissed(): boolean {
  if (typeof window === "undefined") return false;
  return localStorage.getItem(STORAGE_KEY) === "1";
}

export function WelcomePanel({ dispatchCount }: { dispatchCount: number }) {
  const [dismissed, setDismissed] = useState(isDismissed);

  if (dismissed || dispatchCount > 0) return null;

  function handleDismiss() {
    localStorage.setItem(STORAGE_KEY, "1");
    setDismissed(true);
  }

  return (
    <section className="welcome-panel" aria-label="Getting started">
      <div className="welcome-header">
        <h2>Welcome to Agent Control Plane</h2>
        <button onClick={handleDismiss} type="button" className="topbar-btn" aria-label="Dismiss welcome panel">
          Dismiss
        </button>
      </div>
      <p className="hero-copy">
        This local console monitors dispatch history, team state, costs, and audit events.
        Get started in 3 steps:
      </p>
      <ol className="welcome-steps">
        <li>
          <strong>Start the engine</strong>
          <div className="command-block">
            <code>ACP_ADMIN_TOKEN=test123 PORT=9999 ./target/debug/engine</code>
          </div>
        </li>
        <li>
          <strong>Create a noop dispatch</strong>
          <div className="command-block">
            <code>{`curl -X POST http://127.0.0.1:9999/api/v1/dispatch \\
  -H "content-type: application/json" \\
  -d '{"raw_request":"Hello world","request_source":"manual"}'`}</code>
          </div>
        </li>
        <li>
          <strong>View your dispatch</strong>
          <p className="hero-copy">Click the Dispatches tab above to see your first dispatch record.</p>
        </li>
      </ol>
    </section>
  );
}
