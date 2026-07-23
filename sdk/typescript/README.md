# Agent Control Plane TypeScript SDK

REST SDK for the local Agent Control Plane API. It does not bind to private Rust engine internals.

```ts
import { AgentControlPlaneClient } from "@token-efficient-agent-harness/agent-control-plane-sdk";

const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080" });
const bundle = await client.dispatch({ raw_request: "Summarize docs", request_source: "api" });
const recentDocsRuns = await client.dispatches({ limit: 25, search: "docs" });
```

## Error handling

Every request method rejects with a standard `Error` when the API is
unreachable or returns a non-2xx status. The error `message` is the API's
`error` field when the response body carries one, otherwise `HTTP <status>`
(for example `"HTTP 404"`).

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
