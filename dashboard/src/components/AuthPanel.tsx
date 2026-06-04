import { useState } from "react";
import { clearStoredToken, getStoredToken, setStoredToken } from "@/lib/api-client";
import { StateBanner } from "./StateBanner";

export function AuthPanel({
  status,
  message,
  onSaved,
}: {
  status: "missing" | "denied" | "offline";
  message: string;
  onSaved: () => void;
}) {
  const [tokenInput, setTokenInput] = useState(getStoredToken() ?? "");

  function handleSave() {
    const trimmed = tokenInput.trim();
    if (trimmed) {
      setStoredToken(trimmed);
    } else {
      clearStoredToken();
    }
    onSaved();
  }

  function handleClear() {
    setTokenInput("");
    clearStoredToken();
    onSaved();
  }

  const title = status === "offline" ? "Engine offline" : "Authentication required";
  const tone = status === "offline" ? "warn" : status === "denied" ? "risk" : "info";

  return (
    <section className="auth-panel">
      <StateBanner title={title} tone={tone}>
        <p>{message}</p>
      </StateBanner>
      {status !== "offline" && (
        <>
          <label className="form-stack" htmlFor="local-api-key">
            <span className="label">Local API Key</span>
            <input
              id="local-api-key"
              type="password"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              placeholder="harness_<64 hex characters>"
            />
          </label>
          <p className="muted">
            Protected mode uses a local API key stored in this browser only.
          </p>
          <div className="flex-end">
            {getStoredToken() && (
              <button onClick={handleClear} type="button" className="risk-action">
                Clear Token
              </button>
            )}
            <button onClick={handleSave} type="button" disabled={!tokenInput.trim()} className="button-primary">
              Save &amp; Retry
            </button>
          </div>
        </>
      )}
      {status === "offline" && (
        <div className="command-block">
          <span className="label">Start local runtime</span>
          <code>ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine</code>
        </div>
      )}
    </section>
  );
}
