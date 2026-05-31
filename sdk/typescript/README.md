# Agent Control Plane TypeScript SDK

REST SDK for the local Agent Control Plane API. It does not bind to private Rust engine internals.

```ts
import { AgentControlPlaneClient } from "@token-efficient-agent-harness/agent-control-plane-sdk";

const client = new AgentControlPlaneClient({ baseUrl: "http://127.0.0.1:8080" });
const bundle = await client.dispatch({ raw_request: "Summarize docs", request_source: "api" });
const recentDocsRuns = await client.dispatches({ limit: 25, search: "docs" });
```
