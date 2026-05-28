# Agent Control Plane Python SDK

REST SDK for the local Agent Control Plane API. It does not bind to private Rust engine internals.

```python
from agent_control_plane_sdk import AgentControlPlaneClient

client = AgentControlPlaneClient("http://127.0.0.1:8080")
bundle = client.dispatch("Summarize docs")
```
