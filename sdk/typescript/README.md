# Agent Control Plane TypeScript SDK

REST SDK for the local Agent Control Plane API. It does not bind to private Rust engine internals.

```ts
import { AgentControlPlaneClient } from "@token-efficient-agent-harness/agent-control-plane-sdk";

const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080" });
const bundle = await client.dispatch({ raw_request: "Summarize docs", request_source: "api" });
const recentDocsRuns = await client.dispatches({ limit: 25, search: "docs" });
```

## Error handling

Request methods reject with a standard `Error` when the API is unreachable or returns a non-2xx response with a parseable JSON body. The error `message` uses the API body's `error` field when present, otherwise `HTTP <status>` (for example, `"HTTP 404"`). Empty non-2xx bodies also use the HTTP-status fallback. A malformed or non-JSON response body may instead surface the underlying parsing error.

```ts
import { AgentControlPlaneClient } from "@token-efficient-agent-harness/agent-control-plane-sdk";

const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080" });

try {
  const bundle = await client.dispatch({ raw_request: "Summarize docs", request_source: "api" });
  console.log(bundle.record.dispatch_id);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`dispatch failed: ${message}`);
}
```
