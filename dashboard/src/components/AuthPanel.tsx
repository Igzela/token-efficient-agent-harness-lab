import { useState } from "react";
import { clearStoredToken, getStoredToken, setStoredToken } from "@/lib/api-client";

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

  const icon = status === "offline" ? "🔌" : "🔑";

  return (
    <section className="card stack" style={{ maxWidth: 480, margin: "16px auto" }}>
      <h2>{icon} {status === "offline" ? "Engine Offline" : "Authentication Required"}</h2>
      <p className="muted">{message}</p>
      {status !== "offline" && (
        <>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: "0.875rem" }}>Local API Key</span>
            <input
              type="password"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              placeholder="acp-..."
              style={{
                padding: "8px 10px",
                borderRadius: "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--panel)",
                color: "var(--ink)",
              }}
            />
          </label>
          <div className="flex-end">
            {getStoredToken() && (
              <button onClick={handleClear} type="button" className="risk-action">
                Clear Token
              </button>
            )}
            <button onClick={handleSave} type="button" disabled={!tokenInput.trim()}>
              Save &amp; Retry
            </button>
          </div>
        </>
      )}
      {status === "offline" && (
        <p className="muted">Start the engine and reload this page.</p>
      )}
    </section>
  );
}
