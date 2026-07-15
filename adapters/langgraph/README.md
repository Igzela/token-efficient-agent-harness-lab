# ACP LangGraph adapter

This package is a bounded external-runtime adapter. It accepts exactly one
`external_runtime_request.v1` JSON object on standard input, performs exactly
one local LangGraph `graph.invoke(...)`, and emits one
`external_runtime_result.v1` JSON object on standard output.

The adapter does not read credentials, call providers, persist checkpoints, or
own scheduling. Live provider results must already have been converted by the
Rust control plane into the typed, content-free `provider_exchange` contract.
Checkpoint persistence and scope authorization also remain Rust-owned.

Run the focused suite without installing the package into the repository:

```bash
uv run --isolated --no-project --with langgraph==1.2.9 \
  python -m unittest discover -s adapters/langgraph/tests -v
```
