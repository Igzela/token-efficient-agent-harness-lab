import { useEffect, useState } from "react";
import {
  ApiError,
  fetchProviderEndpoints,
  fetchProviderHealth,
  fetchToolPolicyResource,
  saveProviderEndpoints,
} from "@/lib/api-client";
import type {
  LocalDashboardState,
  ProviderEndpointConfig,
  ProviderEndpointConfigResponse,
  ToolPolicyResourceResponse,
} from "@/lib/types";
import { EmptyState } from "./EmptyState";
import { Metric } from "./Metric";
import { StateBanner } from "./StateBanner";

const envVars = [
  { name: "ACP_ADMIN_API_KEY", desc: "Admin API key for protected endpoints" },
  { name: "ACP_REQUIRE_AUTH", desc: "Enable authentication (1 = required)" },
  { name: "ACP_PROVIDER_TYPE", desc: "Provider adapter (openai, anthropic, stub)" },
  { name: "ACP_ENABLE_PROVIDER_EXECUTION", desc: "Enable real provider calls (1 = on)" },
  { name: "ACP_DATABASE_URL", desc: "PostgreSQL connection string (optional)" },
  { name: "ACP_DB_PATH", desc: "SQLite database path" },
  { name: "ACP_BACKUP_INTERVAL_SEC", desc: "Automated backup interval in seconds" },
  { name: "ACP_TLS_CERT_PATH", desc: "TLS certificate path for HTTPS" },
  { name: "ACP_TLS_KEY_PATH", desc: "TLS private key path for HTTPS" },
  { name: "ACP_DB_ENCRYPTION_KEY", desc: "SQLCipher encryption key" },
];

export function Settings({ dashboard }: { dashboard: LocalDashboardState }) {
  const [providerHealth, setProviderHealth] = useState<Record<string, unknown> | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);
  const [endpointConfig, setEndpointConfig] = useState<ProviderEndpointConfigResponse | null>(null);
  const [endpointJson, setEndpointJson] = useState("");
  const [endpointMessage, setEndpointMessage] = useState<string | null>(null);
  const [endpointError, setEndpointError] = useState<string | null>(null);
  const [savingEndpoints, setSavingEndpoints] = useState(false);
  const [toolPolicyKind, setToolPolicyKind] = useState<"capability" | "allowlist" | "hook">("capability");
  const [toolPolicyId, setToolPolicyId] = useState("");
  const [toolPolicy, setToolPolicy] = useState<ToolPolicyResourceResponse | null>(null);
  const [toolPolicyError, setToolPolicyError] = useState<string | null>(null);
  const [loadingToolPolicy, setLoadingToolPolicy] = useState(false);

  useEffect(() => {
    fetchProviderHealth()
      .then((r) => { setProviderHealth(r); setProviderError(null); })
      .catch((e) => {
        if (e instanceof ApiError && (e.status === 401 || e.status === 403)) {
          setProviderError(e.status === 403
            ? "The current API key lacks health:read scope."
            : "Provider health requires local API access.");
        } else {
          setProviderError(e instanceof Error ? e.message : "Failed to load provider health");
        }
      });
    fetchProviderEndpoints()
      .then((r) => {
        setEndpointConfig(r);
        setEndpointJson(JSON.stringify(r.endpoints, null, 2));
        setEndpointError(null);
      })
      .catch((e) => {
        if (e instanceof ApiError && (e.status === 401 || e.status === 403)) {
          setEndpointError(e.status === 403
            ? "The current API key lacks config:read scope."
            : "Provider endpoint config requires local API access.");
        } else {
          setEndpointError(e instanceof Error ? e.message : "Failed to load provider endpoints");
        }
      });
  }, []);

  async function handleSaveProviderEndpoints() {
    setSavingEndpoints(true);
    setEndpointMessage(null);
    setEndpointError(null);
    try {
      const parsed = JSON.parse(endpointJson) as unknown;
      if (!Array.isArray(parsed)) {
        throw new Error("Endpoint config must be a JSON array.");
      }
      const response = await saveProviderEndpoints({
        confirm_provider_endpoint_config: true,
        endpoints: parsed as ProviderEndpointConfig[],
      });
      setEndpointConfig(response);
      setEndpointJson(JSON.stringify(response.endpoints, null, 2));
      setEndpointMessage("Provider endpoint config saved.");
    } catch (e) {
      if (e instanceof ApiError) {
        setEndpointError(e.code ? `${e.code}: ${e.message}` : e.message);
      } else {
        setEndpointError(e instanceof Error ? e.message : "Failed to save provider endpoints");
      }
    } finally {
      setSavingEndpoints(false);
    }
  }

  async function handleInspectToolPolicy() {
    if (!toolPolicyId.trim()) {
      setToolPolicyError("Enter a capability, profile, or hook ID.");
      return;
    }
    setLoadingToolPolicy(true);
    setToolPolicy(null);
    setToolPolicyError(null);
    try {
      setToolPolicy(await fetchToolPolicyResource(toolPolicyKind, toolPolicyId.trim()));
    } catch (error) {
      setToolPolicyError(
        error instanceof ApiError && error.code
          ? `${error.code}: ${error.message}`
          : error instanceof Error
            ? error.message
            : "Failed to inspect tool policy",
      );
    } finally {
      setLoadingToolPolicy(false);
    }
  }

  return (
    <section className="card stack">
      <h2>Settings</h2>
      <div className="stack readable-list">
        {Object.entries(dashboard.config).length === 0 ? (
          <EmptyState
            title="No local config overrides"
            description="This runtime is using default local settings. Config values will appear here once they are persisted in app-owned state."
            tone="info"
          />
        ) : (
          Object.entries(dashboard.config).map(([key, value]) => (
            <div key={key} className="kv-row">
              <span className="muted">{key}</span>
              <span className="mono">{String(value)}</span>
            </div>
          ))
        )}
      </div>
      <h3 className="section-subhead">Provider Health</h3>
      {providerError ? (
        <StateBanner title="Provider health unavailable" tone="warn">
          <p>{providerError}</p>
        </StateBanner>
      ) : providerHealth ? (
        <ProviderHealthSummary providerHealth={providerHealth} />
      ) : (
        <p className="muted"><span className="spinner" />Loading...</p>
      )}
      <h3 className="section-subhead">Adaptive Provider Endpoints</h3>
      <ProviderEndpointConfigPanel
        config={endpointConfig}
        endpointJson={endpointJson}
        error={endpointError}
        message={endpointMessage}
        saving={savingEndpoints}
        onChange={setEndpointJson}
        onSave={handleSaveProviderEndpoints}
      />
      <h3 className="section-subhead">Tool Policy Inspector</h3>
      <div className="subcard stack">
        <p className="muted">
          Read one app-owned policy resource and its current hash. Mutations remain explicit API/SDK operations.
        </p>
        <div className="flex-row" style={{ gap: "0.5rem", flexWrap: "wrap" }}>
          <select
            aria-label="Tool policy resource kind"
            value={toolPolicyKind}
            onChange={(event) => setToolPolicyKind(event.target.value as "capability" | "allowlist" | "hook")}
          >
            <option value="capability">Capability</option>
            <option value="allowlist">Allowlist profile</option>
            <option value="hook">Hook</option>
          </select>
          <input
            aria-label="Tool policy resource ID"
            placeholder="Resource ID"
            value={toolPolicyId}
            onChange={(event) => setToolPolicyId(event.target.value)}
          />
          <button type="button" disabled={loadingToolPolicy} onClick={handleInspectToolPolicy}>
            {loadingToolPolicy ? "Loading..." : "Inspect"}
          </button>
        </div>
        {toolPolicyError ? (
          <StateBanner title="Tool policy unavailable" tone="warn"><p>{toolPolicyError}</p></StateBanner>
        ) : null}
        {toolPolicy ? (
          <div className="stack">
            <div className="kv-row">
              <span className="muted">SHA-256</span>
              <code>{toolPolicy.resource.resource_sha256}</code>
            </div>
            <pre className="command-block">{JSON.stringify(toolPolicy.resource.value, null, 2)}</pre>
          </div>
        ) : null}
      </div>
      <h3 className="section-subhead">Environment Variables</h3>
      <div className="stack readable-list">
        {envVars.map((v) => (
          <div key={v.name} className="kv-row">
            <span className="muted mono" style={{ fontSize: "12px" }}>{v.name}</span>
            <span style={{ fontSize: "13px" }}>{v.desc}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function ProviderEndpointConfigPanel({
  config,
  endpointJson,
  error,
  message,
  saving,
  onChange,
  onSave,
}: {
  config: ProviderEndpointConfigResponse | null;
  endpointJson: string;
  error: string | null;
  message: string | null;
  saving: boolean;
  onChange: (value: string) => void;
  onSave: () => void;
}) {
  if (error && !config) {
    return (
      <StateBanner title="Provider endpoint config unavailable" tone="warn">
        <p>{error}</p>
      </StateBanner>
    );
  }

  const source = config?.source ?? "loading";
  const endpoints = config?.endpoints ?? [];
  const runtime = config?.runtime;
  const completionExecutorConfigured =
    runtime?.completion_executor_configured ?? runtime?.executor_configured;
  const completionRegistryConfigured =
    runtime?.completion_registry_configured ?? runtime?.registry_configured;
  const workflowExecutorConfigured =
    runtime?.workflow_executor_configured ?? runtime?.executor_configured;
  const completionDetail = runtime?.local_config_error_code === "environment_override_active"
    ? "environment config active"
    : runtime?.local_config_error_code
      ? `blocked: ${runtime.local_config_error_code}`
    : runtime?.local_config_applies_to_completion_api
      ? "completion API live"
      : runtime?.local_config_apply_requires_restart
        ? "restart/reload needed"
      : "current runtime";

  return (
    <div className="stack">
      <StateBanner title="Secrets stay outside dashboard config" tone="info">
        <p>
          Configure provider endpoints with symbolic credential env names only. Raw API keys are rejected by the engine and must remain in the local environment.
        </p>
      </StateBanner>
      <div className="metrics">
        <Metric label="Source" value={source} detail={`${endpoints.length} endpoint(s)`} />
        <Metric
          label="Completion"
          value={completionExecutorConfigured ? "configured" : "missing"}
          detail={completionDetail}
          tone={completionExecutorConfigured ? "ok" : "info"}
        />
        <Metric
          label="Registry"
          value={completionRegistryConfigured ? "configured" : "missing"}
          detail={workflowExecutorConfigured ? "workflow runtime active" : "completion-only local config"}
          tone={completionRegistryConfigured ? "ok" : "info"}
        />
      </div>
      {message ? (
        <StateBanner title="Provider endpoints saved" tone="ok">
          <p>{message}</p>
        </StateBanner>
      ) : null}
      {error ? (
        <StateBanner title="Provider endpoint config rejected" tone="warn">
          <p>{error}</p>
        </StateBanner>
      ) : null}
      <label className="muted" htmlFor="provider-endpoint-config">
        Endpoint JSON
      </label>
      <textarea
        id="provider-endpoint-config"
        value={endpointJson}
        onChange={(event) => onChange(event.target.value)}
        rows={14}
        placeholder={JSON.stringify([{
          endpoint_id: "openai-quality",
          provider_type: "openai_compatible",
          base_url: "https://api.openai.example/v1",
          model: "quality-model",
          credential_env: "OPENAI_QUALITY_KEY",
          timeout_ms: 30000,
          input_cost_per_1k_usd: 0.01,
          output_cost_per_1k_usd: 0.03,
        }], null, 2)}
      />
      <div className="flex-end">
        <button type="button" className="button-primary" onClick={onSave} disabled={saving}>
          {saving ? "Saving..." : "Save provider endpoints"}
        </button>
      </div>
    </div>
  );
}

function ProviderHealthSummary({ providerHealth }: { providerHealth: Record<string, unknown> }) {
  const status = String(providerHealth.status ?? "unknown");
  const enabled = providerHealth.enabled === true;
  const providerId = providerHealth.provider_id ? String(providerHealth.provider_id) : "none";
  const message = providerHealth.message ? String(providerHealth.message) : null;
  const tone = status === "ok" ? "ok" : status === "noop" ? "info" : "warn";

  return (
    <div className="stack">
      <StateBanner title={status === "ok" ? "Provider transport is enabled" : "Provider transport is not active"} tone={tone}>
        <p>
          {message ?? (enabled
            ? "A provider adapter is configured for this local runtime."
            : "No real provider calls are configured for this local runtime. Dispatches stay on noop or explicit opt-in paths.")}
        </p>
      </StateBanner>
      <div className="metrics">
        <div className="metric">
          <span className="metric-label">Provider</span>
          <strong>{providerId}</strong>
          <span className={tone}>{status}</span>
        </div>
        <div className="metric">
          <span className="metric-label">Execution</span>
          <strong>{enabled ? "enabled" : "off"}</strong>
          <span className={enabled ? "warn" : "ok"}>{enabled ? "explicit" : "default-safe"}</span>
        </div>
      </div>
      <details>
        <summary>Raw provider response</summary>
        <pre>{JSON.stringify(providerHealth, null, 2)}</pre>
      </details>
    </div>
  );
}
